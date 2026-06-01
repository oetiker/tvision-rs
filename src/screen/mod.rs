//! Screen-cell storage and (later) the render back-buffer + diff.
//!
//! Row 6 lands [`Cell`] now. The `DrawBuffer` (row 7), the vendored ratatui
//! `Buffer` + cell diff (row 18, D8), and the whole-tree redraw engine follow in
//! this module.

mod buffer;
mod cell;
mod draw_buffer;
pub mod snapshot;

pub use buffer::Buffer;
pub use cell::Cell;
pub use draw_buffer::DrawBuffer;
