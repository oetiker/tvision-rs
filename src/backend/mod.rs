//! The platform seam: everything that touches the terminal lives behind a
//! single trait, so the rest of the framework stays terminal-agnostic.
//!
//! ## Color quantization
//!
//! Pure color-mapping math: `RGB → xterm-256 → xterm-16 → BIOS`. All functions
//! are const-capable and I/O-free, so a backend can downsample any color to the
//! depth its terminal supports.
//!
//! ## Backend trait + implementations
//!
//! The [`Backend`] trait is the seam between the framework and the terminal.
//! Two implementations are provided:
//!
//! - [`CrosstermBackend`] — production, wraps crossterm.
//! - [`HeadlessBackend`] + [`HeadlessHandle`] — tests, in-memory buffer.
//!
//! [`Renderer`] owns the back/front buffer pair and a boxed backend; it runs the
//! draw cycle (reset → paint → diff → draw → flush → swap), painting the whole
//! view tree each frame and relying on the diff to keep terminal output minimal.

mod clipboard;
mod crossterm_backend;
mod headless;
mod quantize;
mod renderer;
mod traits;

pub use crossterm_backend::{ColorDepth, CrosstermBackend};
pub use headless::{HeadlessBackend, HeadlessHandle};
pub use quantize::*;
pub use renderer::Renderer;
pub use traits::Backend;
