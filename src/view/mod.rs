//! Views, geometry, and the downward-context substrate.
//!
//! This module will eventually hold the `View` trait + `ViewState` (`TView`,
//! D2/D5), the `ViewId` generational arena (D3, row 17), and the `Context` /
//! `DrawCtx` downward-context types (D3, row 22). For now it carries the
//! geometry primitives every later row depends on.

mod geometry;
mod id;

pub use geometry::{Point, Rect};
pub use id::{ViewArena, ViewId};
