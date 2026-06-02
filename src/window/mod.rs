//! `TWindow` — the framed, selectable, movable window (row 33).
//!
//! Row 30 [`Desktop`](crate::desktop::Desktop) gave `Program` a named desktop;
//! row 33 [`Window`] is the thing a user actually sees and drives: a [`Group`]
//! that builds a [`Frame`](crate::frame::Frame) around itself, owns a window
//! number / title / decoration flags, can hold standard scroll bars, and (in
//! later stages) zooms / moves / resizes / closes.
//!
//! This module is **stage 33b** — the *static, selectable* window core. The
//! drag / zoom / close behaviour and the `setState` command-enable block defer
//! to **stage 33c** (each needs infrastructure not present yet: an
//! owner-extent-down channel, drag capture handlers, a close-removal channel).
//! See the breadcrumbs in [`window`](self::window).
//!
//! ## Deviations in play
//! * **D2** embed-and-delegate: [`Window`] embeds a [`Group`] and forwards the
//!   [`View`](crate::view::View) trait to it, overriding only `draw`/
//!   `handle_event`/`set_state`/`size_limits` where `TWindow` does.
//! * **D3** owner-data-down: no owner back-pointer; the frame's title/flags/
//!   number are pushed **down** at construction. (zoom/drag's owner-extent need
//!   is 33c.)
//! * **D4** `Event::Broadcast` carries a `source: ViewId` (the broadcast-subject
//!   successor to `infoPtr`); the `cmSelectWindowNum` window-number match still
//!   **defers to 33d** — its blocker is the missing `select()`/`canMoveFocus`
//!   machinery, not a payload story (the window number is an *integer* argument,
//!   not a `ViewId`, so `source` does not serve it; Alt-N is a direct walk).
//! * **D5** the `wf*` flag word → [`WindowFlags`] (relocated here from
//!   `frame.rs`, where it lived because `Frame` was the first thing to need it);
//!   `WindowPalette` for the `palette` member.
//! * **D7** no `getPalette`; [`WindowPalette`] records the colour scheme and the
//!   single (blue) scheme renders via the existing `Frame` roles. Mapping
//!   `Cyan`/`Gray` to distinct theme roles is **row 34's** job (`TDialog` uses
//!   `Gray`).
//! * **D8** whole-tree redraw; shadow casting still deferred (the `group.rs`
//!   `// TODO(row 33)`).

#[allow(clippy::module_inception)]
mod window;

pub use window::{ScrollBarOptions, Window, WindowFlags, WindowPalette};
