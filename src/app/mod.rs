//! The application layer: [`Program`], the application root that owns the
//! single event loop, the timer queue, and the capture stack that powers
//! modal dialogs; and [`Application`], a thin wrapper over it that adds
//! window tiling/cascading and shell suspend.
//!
//! **Guide:** [The event loop in depth](../../../internals/event-loop.html).
//!
//! # Turbo Vision heritage
//! Ports `TProgram` / `TApplication` (`tprogram.cpp`, `tapplica.cpp`). The
//! `TApplication : TProgram` inheritance becomes embed-and-delegate composition
//! (deviation D2) — one type holds the other and forwards to it.

mod application;
mod program;

pub use application::Application;
pub use program::{ModalFrame, Program};
