//! Views, geometry, and the downward-context substrate.
//!
//! Carries the geometry primitives ([`Point`]/[`Rect`]), the global [`ViewId`]
//! minter ([`id`]), the downward [`Context`] / [`DrawCtx`] types ([`context`]),
//! and the [`View`] trait + [`ViewState`] that every widget builds on.
//!
//! **Guide:** [The view tree](../../../internals/view-tree.html).

mod context;
mod geometry;
// `view::group` houses `TGroup`: the child tree + three-phase routing
// + focus machinery. Re-exported below alongside the trait it builds on.
mod group;
mod id;
// `view::view` houses the `View` trait + `ViewState` (TView). The
// inner name mirrors the C++ class file; the re-exports below flatten it away.
#[allow(clippy::module_inception)]
mod view;

pub use context::{Context, Deferred, DrawCtx};
pub use geometry::{Point, Rect};
pub use group::{Group, SelectMode};
pub use id::ViewId;
/// `TView::locate` free function — backs `Desktop::tile`/`cascade` (see its doc).
pub(crate) use view::locate;
pub use view::{DragMode, GrowMode, Options, Phase, State, StateFlag, View, ViewState};
