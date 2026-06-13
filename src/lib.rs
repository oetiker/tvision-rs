//! # rstv — a text-user-interface framework for Rust
//!
//! rstv is an idiomatic Rust port of [magiblot/tvision](https://github.com/magiblot/tvision),
//! a modern revival of Turbo Vision. It gives you the classic Turbo Vision
//! experience — overlapping windows, modal dialogs, menus, a status line,
//! validated input fields, scrollers and list boxes — built on a retained view
//! tree and a single event loop, rendered to the terminal through a pluggable
//! [`Backend`].
//!
//! ## House style: the `tv::` namespace
//!
//! Add the crate to `Cargo.toml` aliased as `tv`, and reach everything through
//! the `tv::` path — the path *is* the namespace, so types drop the `T` prefix
//! the original used in their names:
//!
//! ```toml
//! # Cargo.toml
//! tv = { package = "rstv", version = "0.1" }
//! ```
//!
//! ```
//! # use rstv as tv;
//! let _r = tv::Rect::new(0, 0, 80, 25);
//! ```
//!
//! Public types are therefore re-exported at the crate root below, even though
//! they live in topical modules internally.
//!
//! ## Tour of the modules
//!
//! - [`view`] — the [`View`] trait and [`ViewState`] that every widget is built
//!   on, plus geometry ([`Point`], [`Rect`]) and the [`Group`] container.
//! - [`app`] — [`Program`], the application root that owns the event loop, and
//!   [`Application`], which adds window tiling/cascading and shell suspend.
//! - [`window`], [`dialog`], [`desktop`] — top-level frames: resizable windows,
//!   modal dialogs, and the backdrop they sit on.
//! - [`widgets`] — buttons, input lines, check boxes, list boxes, scrollers,
//!   editors, the outline viewer, and more.
//! - [`menu`], [`status`] — the menu bar / pull-down menus and the status line.
//! - [`event`], [`command`] — the [`Event`] enum and key/mouse types, and the
//!   command identifiers that drive them.
//! - [`color`], [`theme`] — colors and styles, and the [`Theme`] that maps a
//!   widget [`Role`] to glyphs and attributes.
//! - [`screen`], [`backend`] — the cell buffer and diffing renderer, and the
//!   [`Backend`] trait with its crossterm and headless implementations.
//! - [`validate`], [`text`], [`timer`], [`capture`], [`data`], [`help`] — input
//!   validators, text measurement, the timer queue, the event-capture stack,
//!   the typed value protocol, and help contexts.
//!
//! # Turbo Vision heritage
//!
//! rstv ports Borland's Turbo Vision as modernized by magiblot/tvision (a C++
//! codebase). The pervasive translation choices — inheritance becomes the
//! [`View`] trait plus [`ViewState`] composition, raw pointers become [`ViewId`]
//! handles, flag words become structs of bools, and the palette becomes a
//! [`Theme`] keyed by [`Role`] — are summarized in the project's guide.

// Lets proc-macro-generated `::rstv::Type` paths resolve inside this crate.
extern crate self as rstv;

pub mod app;
pub mod backend;
pub mod capture;
pub mod color;
pub mod command;
pub mod data;
pub mod desktop;
pub mod dialog;
pub mod event;
pub mod frame;
pub mod help;
pub mod keymap;
pub mod menu;
pub mod screen;
pub mod status;
pub mod text;
pub mod theme;
pub mod timer;
pub mod validate;
pub mod view;
pub mod widgets;
pub mod window;

// --- House-style root re-exports (so `tv::Point` etc. resolve without `use`) ---

pub use app::{Application, ModalFrame, Program};
pub use backend::{Backend, CrosstermBackend, HeadlessBackend, HeadlessHandle, Renderer};
pub use capture::{CaptureFlow, CaptureHandler, CaptureStack, TrackMask};
pub use color::{Color, Modifiers, Style};
pub use command::{Command, CommandSet};
pub use data::FieldValue;
pub use desktop::{Background, Desktop};
pub use dialog::{
    CD_HELP_BUTTON, CD_NO_LOAD_DIR, CD_NORMAL, ChDirDialog, ColorPicker, Dialog, DirCollection,
    DirEntry, DirListBox, FA_DIREC, FD_CLEAR_BUTTON, FD_HELP_BUTTON, FD_NO_LOAD_DIR, FD_OK_BUTTON,
    FD_OPEN_BUTTON, FD_REPLACE_BUTTON, FileCollection, FileDialog, FileInfoPane, FileInputLine,
    FileList, MessageBoxButtons, MessageBoxKind, SearchRec, Tab, search_rec_compare,
};
pub use event::{
    Event, EventMask, Key, KeyEvent, KeyModifiers, MouseButtons, MouseEvent, MouseEventFlags,
    MouseWheel, ctrl_to_arrow, hot_key, is_alt_hotkey, is_plain_hotkey,
};
pub use frame::Frame;
pub use help::HelpCtx;
pub use keymap::{KeyStroke, Keymap, Resolve};
pub use menu::{
    Menu, MenuBar, MenuBox, MenuBuilder, MenuItem, MenuView, MenuViewState, alt, popup_menu,
};
pub use rstv_macros::delegate;
pub use screen::{Buffer, Cell, DrawBuffer};
pub use status::{HelpCtxRange, StatusColors, StatusDef, StatusItem, StatusLine};
pub use theme::{Role, Theme};
pub use timer::{Clock, ManualClock, SystemClock, TimerId, TimerQueue};
pub use validate::{
    FilterValidator, LookupValidator, PXPictureValidator, RangeValidator, RegexError,
    RegexValidator, StringLookupValidator, Validator,
};
pub use view::{
    Context, DragMode, DrawCtx, Group, GrowMode, Options, Phase, Point, Rect, SelectMode, State,
    StateFlag, View, ViewId, ViewState,
};
pub use widgets::Indicator;
pub use widgets::{Button, ButtonFlags};
pub use widgets::{CheckBoxes, Cluster, ClusterKind, MultiCheckBoxes, RadioButtons};
pub use widgets::{
    EditWindow, Editor, InputLine, LimitMode, ListBox, ListRoles, ListViewer, ListViewerState,
    ScrollBar, Scroller, SortedListBox,
};
pub use widgets::{Encoding, FileEditor, LineEnding, Memo};
pub use widgets::{
    HistoryViewer, HistoryWindow, THistory, clear_history, history_add, history_count, history_str,
};
pub use widgets::{Label, ParamText, StaticText};
pub use widgets::{Node, Outline, OutlineViewer, OutlineViewerState, ov_update};
pub use widgets::{Terminal, TextDevice};
pub use window::{ScrollBarOptions, Window, WindowFlags, WindowPalette};
