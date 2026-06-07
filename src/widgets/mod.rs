//! Widget implementations — leaf views ported from Turbo Vision (Phase 1+).
//!
//! Each submodule holds one widget class (dropping the `T` prefix per D1).
//! The canonical embed pattern is: `state: ViewState`, `impl View` returning
//! it from `state`/`state_mut`, implement `draw` through [`DrawCtx`], and
//! handle events through [`Context`].
//!
//! [`DrawCtx`]: crate::view::DrawCtx
//! [`Context`]: crate::view::Context

mod button;
mod cluster;
mod editor;
mod history;
mod indicator;
mod input_line;
mod list_box;
pub mod list_viewer;
mod scrollbar;
mod scroller;
mod static_text;

pub use button::{Button, ButtonFlags};
pub use cluster::{CheckBoxes, Cluster, ClusterKind, MultiCheckBoxes, RadioButtons};
pub use editor::{Editor, Encoding, LineEnding};
pub use history::{
    HistoryViewer, HistoryWindow, THistory, clear_history, history_add, history_count, history_str,
};
pub use indicator::Indicator;
pub use input_line::{InputLine, LimitMode};
pub use list_box::ListBox;
pub use list_viewer::{ListViewer, ListViewerState};
pub use scrollbar::ScrollBar;
pub use scroller::Scroller;
pub use static_text::{Label, ParamText, StaticText};
