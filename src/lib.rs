//! # tvision — an idiomatic Rust port of Turbo Vision
//!
//! A faithful Rust port of [magiblot/tvision](https://github.com/magiblot/tvision)
//! (modern C++ Turbo Vision). We port *faithfully* from the C++ source; the only
//! intentional departures are the pre-decided deviations `D1`–`D13` documented in
//! `docs/PORTING-GUIDE.md`. The dependency-ordered class list is in
//! `docs/PORT-ORDER.md`.
//!
//! ## House style (`D1`)
//!
//! Consumers alias the crate as `tv` and reach everything through the `tv::`
//! path — the path *is* the namespace the old `T` prefix was faking:
//!
//! ```toml
//! # Cargo.toml
//! tv = { package = "tvision", version = "0.1" }
//! ```
//!
//! ```ignore
//! let r = tv::Rect::new(0, 0, 80, 25);
//! ```
//!
//! Public types are therefore re-exported at the crate root below, even though
//! they live in topical modules internally.
//!
//! ## Phase 0 substrate (this milestone)
//!
//! Per `docs/PORT-ORDER.md`, Phase 0 is the primitives + net-new runtime/render
//! substrate. Rows land in dependency order:
//!
//! | row | item | module | status |
//! |-----|------|--------|--------|
//! | 1, 2 | `Point`, `Rect` | [`view`] (geometry) | ✅ |
//! | 3, 4 | `Color`, `Style` | [`color`] | ✅ |
//! | 6 | `Cell` | [`screen`] | ✅ |
//! | 8 | `Text` (width/scroll) | [`text`] | ✅ |
//! | 7 | `DrawBuffer` | [`screen`] | ✅ |
//! | 5 | quantization ladder | `backend` | ⏳ |
//! | 9 | glyph tables | `theme` | ⏳ |
//! | 10 | `Key` | [`event`] | ✅ |
//! | 11 | `Event` | [`event`] | ✅ |
//! | 12 | `Command` / command set | [`command`] | ✅ |
//! | 16 | `Theme` | `theme` | ⏳ |
//! | 17 | `ViewId` arena | [`view`] | ⏳ |
//! | 18 | back-buffer + diff | `screen` | ⏳ |
//! | 19 | `Backend` (+ crossterm/headless) | `backend` | ⏳ |
//! | 20 | `Clock` + timer queue | `timer` | ⏳ |
//! | 21 | capture stack | `capture` | ⏳ |
//! | 22 | `Context` / `DrawCtx` | [`view`] | ⏳ |

pub mod color;
pub mod command;
pub mod event;
pub mod screen;
pub mod text;
pub mod view;

// --- House-style root re-exports (so `tv::Point` etc. resolve without `use`) ---

pub use color::{Color, Modifiers, Style};
pub use command::{Command, CommandSet};
pub use event::{
    Event, EventMask, Key, KeyEvent, KeyModifiers, MouseButtons, MouseEvent, MouseEventFlags,
    MouseWheel,
};
pub use screen::{Cell, DrawBuffer};
pub use view::{Point, Rect};
