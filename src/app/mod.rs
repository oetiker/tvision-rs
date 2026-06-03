//! Application layer — `TProgram` (row 31) and `TApplication` (row 32).
//!
//! [`Program`] is the application root: it owns TV's single event loop (D9),
//! making the row-20 timer queue and the row-21 capture stack live. See
//! [`program`] for the module docs and the deferral breadcrumbs.
//!
//! [`Application`] is a thin D2 embed-and-delegate wrapper over [`Program`]
//! that adds (deferred) `tile`/`cascade`/`dosShell` and `get_tile_rect`.

mod application;
mod program;

pub use application::Application;
pub use program::{ModalFrame, Program};
