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
//!
//! ## Deliberate omissions
//!
//! Turbo Vision's `TMonoSelector` — a cluster-based picker for monochrome
//! screen attributes, used only inside the old color dialog — is intentionally
//! not ported. The color picker was rebuilt (see [`crate::dialog::ColorPicker`])
//! and needs no mono-attribute selector; the general tabbed-selection idiom is
//! covered by [`TabBar`](crate::widgets::TabBar).

mod button;
mod cluster;
mod editor;
mod history;
mod indicator;
mod input_line;
mod list_box;
pub mod list_viewer;
mod masked_input;
pub mod outline;
mod page_stack;
mod reveal_eye;
mod scrollbar;
mod scroller;
pub mod splitter;
mod static_text;
mod tab_bar;
pub mod terminal;

pub use button::{Button, ButtonFlags};
pub use cluster::{CheckBoxes, Cluster, ClusterKind, MultiCheckBoxes, RadioButtons};
pub(crate) use editor::EF_DO_REPLACE;
pub(crate) use editor::editor_mut;
pub use editor::{
    EditWindow, Editor, Encoding, FileEditor, LineEnding, Memo, SM_DOUBLE, SM_EXTEND, SM_TRIPLE,
};
pub use history::{
    HistoryViewer, HistoryWindow, THistory, clear_history, history_add, history_count, history_str,
};
pub use indicator::Indicator;
pub use input_line::{InputLine, LimitMode, ValuePosition};
pub use list_box::{ListBox, SortedListBox};
pub use list_viewer::{FindMode, ListRoles, ListViewer, ListViewerState};
pub use masked_input::MaskedInput;
pub use outline::{Node, Outline, OutlineViewer, OutlineViewerState, ov_update};
pub use page_stack::PageStack;
pub use reveal_eye::{RevealEye, RevealEyeConfig};
pub use scrollbar::ScrollBar;
pub use scroller::Scroller;
pub use splitter::{Constraints, DividerStyle, Orientation, Splitter};
pub use static_text::{Label, ParamText, StaticText};
pub use tab_bar::TabBar;
pub use terminal::{Terminal, TextDevice};
