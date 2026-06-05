//! Views, geometry, and the downward-context substrate.
//!
//! Carries the geometry primitives ([`Point`]/[`Rect`], rows 1–2), the
//! global [`ViewId`] minter ([`id`], D3 row 17), and the downward
//! [`Context`] / [`DrawCtx`] types ([`context`], D3/D4 row 22). The `View` trait
//! + `ViewState` (`TView`, D2/D5) land here in Phase 1 (row 23).

mod context;
mod geometry;
// `view::group` houses `TGroup` (row 26): the child tree + three-phase routing
// + focus machinery. Re-exported below alongside the trait it builds on.
mod group;
mod id;
// `view::view` houses the `View` trait + `ViewState` (TView, row 23). The
// inner name mirrors the C++ class file; the re-exports below flatten it away.
#[allow(clippy::module_inception)]
mod view;

pub use context::{Context, Deferred, DrawCtx};
pub use geometry::{Point, Rect};
pub use group::{Group, SelectMode};
pub use id::ViewId;
/// `TView::locate` free function — backs `Desktop::tile`/`cascade` (see its doc).
pub(crate) use view::locate;
pub use view::{DragMode, GrowMode, Options, State, StateFlag, View, ViewState};
