//! Views, geometry, and the downward-context substrate.
//!
//! Carries the geometry primitives ([`Point`]/[`Rect`], rows 1–2), the
//! [`ViewId`] generational arena ([`id`], D3 row 17), and the downward
//! [`Context`] / [`DrawCtx`] types ([`context`], D3/D4 row 22). The `View` trait
//! + `ViewState` (`TView`, D2/D5) land here in Phase 1 (row 23).

mod context;
mod geometry;
mod id;
// `view::view` houses the `View` trait + `ViewState` (TView, row 23). The
// inner name mirrors the C++ class file; the re-exports below flatten it away.
#[allow(clippy::module_inception)]
mod view;

pub use context::{Context, DrawCtx};
pub use geometry::{Point, Rect};
pub use id::{ViewArena, ViewId};
pub use view::{DragMode, GrowMode, Options, State, View, ViewState};
