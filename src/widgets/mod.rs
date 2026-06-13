//! Widget implementations — the leaf views (buttons, input lines, lists,
//! editors, scrollbars, and more).
//!
//! Each submodule holds one widget. The canonical embed pattern is:
//! `state: ViewState`, `impl View` returning it from `state`/`state_mut`,
//! implement `draw` through [`DrawCtx`], and handle events through [`Context`].
//!
//! [`DrawCtx`]: crate::view::DrawCtx
//! [`Context`]: crate::view::Context
//!
//! **Guide:** [Controls](../../../apps/controls.html).

mod button;
mod cluster;
mod editor;
mod history;
mod indicator;
mod input_line;
mod list_box;
pub mod list_viewer;
pub mod outline;
mod scrollbar;
mod scroller;
pub mod splitter;
mod static_text;
pub mod terminal;

pub use button::{Button, ButtonFlags};
pub use cluster::{CheckBoxes, Cluster, ClusterKind, MultiCheckBoxes, RadioButtons};
pub(crate) use editor::EF_DO_REPLACE;
pub(crate) use editor::editor_mut;
pub use editor::{EditWindow, Editor, Encoding, FileEditor, LineEnding, Memo};
pub use history::{
    HistoryViewer, HistoryWindow, THistory, clear_history, history_add, history_count, history_str,
};
pub use indicator::Indicator;
pub use input_line::{InputLine, LimitMode};
pub use list_box::{ListBox, SortedListBox};
pub use list_viewer::{ListRoles, ListViewer, ListViewerState};
pub use outline::{Node, Outline, OutlineViewer, OutlineViewerState, ov_update};
pub use scrollbar::ScrollBar;
pub use scroller::Scroller;
pub use static_text::{Label, ParamText, StaticText};
pub use terminal::{Terminal, TextDevice};
