//! Application layer — `TProgram` and the live event loop (row 31).
//!
//! [`Program`] is the application root: it owns TV's single event loop (D9),
//! making the row-20 timer queue and the row-21 capture stack live. See
//! [`program`] for the module docs and the deferral breadcrumbs.

mod program;

pub use program::{ModalFrame, Program};
