//! Platform seam — deviations **D6** (row 5) and **D11** (row 19).
//!
//! ## Row 5 — quantization ladder
//!
//! Pure color-mapping math: `RGB → xterm-256 → xterm-16 → BIOS`.
//! All functions are const-capable and I/O-free.
//!
//! ## Row 19 — Backend trait + impls
//!
//! The [`Backend`] trait is the seam between the framework and the terminal.
//! Two implementations are provided:
//!
//! - [`CrosstermBackend`] — production, wraps crossterm.
//! - [`HeadlessBackend`] + [`HeadlessHandle`] — tests, in-memory buffer.
//!
//! [`Renderer`] owns the back/front buffer pair and a boxed backend; it runs
//! the D8 draw cycle (reset → paint → diff → draw → flush → swap).

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
