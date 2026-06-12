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
//! # Turbo Vision heritage
//!
//! Ports `TWindow`. C++ `TWindow : TGroup` inheritance becomes embed-and-
//! delegate composition (deviation D2): [`Window`] embeds a [`Group`] and
//! forwards the [`View`](crate::view::View) trait to it, overriding only the
//! methods `TWindow` does. A window never reaches up to an owner — the frame's
//! title/flags/number are pushed down (deviation D3); the `wf*` flag word
//! becomes [`WindowFlags`] (deviation D5); and `getPalette` becomes a
//! [`WindowPalette`] → [`Role`](crate::theme::Role) mapping (deviation D7).

#[allow(clippy::module_inception)]
mod window;

pub use window::{ScrollBarOptions, Window, WindowFlags, WindowPalette};
