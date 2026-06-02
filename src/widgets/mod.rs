//! Widget implementations — leaf views ported from Turbo Vision (Phase 1+).
//!
//! Each submodule holds one widget class (dropping the `T` prefix per D1).
//! The canonical embed pattern is: `state: ViewState`, `impl View` returning
//! it from `state`/`state_mut`, implement `draw` through [`DrawCtx`], and
//! handle events through [`Context`].
//!
//! [`DrawCtx`]: crate::view::DrawCtx
//! [`Context`]: crate::view::Context

mod cluster;
mod scrollbar;
mod static_text;

pub use cluster::{CheckBoxes, Cluster, ClusterKind, MultiCheckBoxes, RadioButtons};
pub use scrollbar::ScrollBar;
pub use static_text::StaticText;
