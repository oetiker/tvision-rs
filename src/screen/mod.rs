//! Screen-cell storage, the render back-buffer, and the cell diff.
//!
//! [`Cell`] is one character cell; [`DrawBuffer`] is a scratch row a view fills
//! and blits; [`Buffer`] is the full in-memory screen grid that the whole view
//! tree paints into each frame, then diffs against the previous frame so only
//! changed cells reach the terminal. The [`snapshot`](crate::screen::snapshot)
//! module renders a `Buffer` to a deterministic golden string for tests.
//!
//! **Guide:** [Drawing & backends](../../../internals/drawing.html).

mod buffer;
mod cell;
mod draw_buffer;
pub mod snapshot;

pub use buffer::Buffer;
pub use cell::Cell;
pub use draw_buffer::DrawBuffer;
