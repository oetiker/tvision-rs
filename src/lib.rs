//! # tvision — an idiomatic Rust port of Turbo Vision
//!
//! A faithful Rust port of [magiblot/tvision](https://github.com/magiblot/tvision)
//! (modern C++ Turbo Vision). We port *faithfully* from the C++ source; the only
//! intentional departures are the pre-decided deviations `D1`–`D13` documented in
//! `docs/PORTING-GUIDE.md`. The dependency-ordered class list is in
//! `docs/PORT-ORDER.md`.
//!
//! ## House style (`D1`)
//!
//! Consumers alias the crate as `tv` and reach everything through the `tv::`
//! path — the path *is* the namespace the old `T` prefix was faking:
//!
//! ```toml
//! # Cargo.toml
//! tv = { package = "tvision", version = "0.1" }
//! ```
//!
//! ```ignore
//! let r = tv::Rect::new(0, 0, 80, 25);
//! ```
//!
//! Public types are therefore re-exported at the crate root below, even though
//! they live in topical modules internally.
//!
//! ## Phase 0 substrate (this milestone)
//!
//! Per `docs/PORT-ORDER.md`, Phase 0 is the primitives + net-new runtime/render
//! substrate. Rows land in dependency order:
//!
//! | row | item | module | status |
//! |-----|------|--------|--------|
//! | 1, 2 | `Point`, `Rect` | [`view`] (geometry) | ✅ |
//! | 3, 4 | `Color`, `Style` | [`color`] | ✅ |
//! | 6 | `Cell` | [`screen`] | ✅ |
//! | 8 | `Text` (width/scroll) | [`text`] | ✅ |
//! | 7 | `DrawBuffer` | [`screen`] | ✅ |
//! | 5 | quantization ladder | [`backend`] | ✅ |
//! | 9 | glyph tables | [`theme`] (stub) | ⏳ |
//! | 10 | `Key` | [`event`] | ✅ |
//! | 11 | `Event` | [`event`] | ✅ |
//! | 12 | `Command` / command set | [`command`] | ✅ |
//! | 16 | `Theme` (minimal) | [`theme`] | ✅ |
//! | 17 | `ViewId` minter | [`view`] | ✅ |
//! | 18 | back-buffer + diff | [`screen`] | ✅ |
//! | 19 | `Backend` (+ crossterm/headless) | [`backend`] | ✅ |
//! | 20 | `Clock` + timer queue | [`timer`] | ✅ |
//! | 21 | capture stack | [`capture`] | ✅ |
//! | 22 | `Context` / `DrawCtx` | [`view`] | ✅ |
//!
//! ## Phase 1 (widgets)
//!
//! | row | item | module | status |
//! |-----|------|--------|--------|
//! | 23 | `TView` (`View` trait + `ViewState`) | [`view`] | ✅ |
//! | 29 | `TBackground` | [`desktop`] | ✅ |
//! | 25 | `TScrollBar` | [`widgets`] | ✅ |
//! | 26 | `TGroup` (`Group`) | [`view`] | ✅ |
//! | 24 | `TFrame` (`Frame`) | [`frame`] | ✅ |
//! | 31 | `TProgram` (`Program`, live loop) | [`app`] | ✅ |
//! | 33 | `TWindow` (`Window`, core) | [`window`] | ✅ |
//! | 30 | `TDeskTop` (`Desktop`) | [`desktop`] | ✅ |
//! | 34 | `TDialog` (`Dialog`, modal `exec_view`) | [`dialog`] | ✅ |
//! | 27 | `TScroller` (`Scroller`, cross-view broker) | [`widgets`] | ✅ |

// Lets proc-macro-generated `::tvision::Type` paths resolve inside this crate.
extern crate self as tvision;

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
pub use capture::{CaptureFlow, CaptureHandler, CaptureStack};
pub use color::{Color, Modifiers, Style};
pub use command::{Command, CommandSet};
pub use data::FieldValue;
pub use desktop::{Background, Desktop};
pub use dialog::{Dialog, MessageBoxButtons, MessageBoxKind};
pub use event::{
    Event, EventMask, Key, KeyEvent, KeyModifiers, MouseButtons, MouseEvent, MouseEventFlags,
    MouseWheel, ctrl_to_arrow, hot_key, is_alt_hotkey, is_plain_hotkey,
};
pub use frame::Frame;
pub use help::HelpCtx;
pub use menu::{
    Menu, MenuBar, MenuBox, MenuBuilder, MenuItem, MenuView, MenuViewState, alt, popup_menu,
};
pub use screen::{Buffer, Cell, DrawBuffer};
pub use status::{HelpCtxRange, StatusColors, StatusDef, StatusItem, StatusLine};
pub use theme::{Role, Theme};
pub use timer::{Clock, ManualClock, SystemClock, TimerId, TimerQueue};
pub use tvision_macros::delegate;
pub use validate::{
    FilterValidator, LookupValidator, PXPictureValidator, RangeValidator, RegexError,
    RegexValidator, StringLookupValidator, Validator,
};
pub use view::{
    Context, DragMode, DrawCtx, Group, GrowMode, Options, Point, Rect, SelectMode, State,
    StateFlag, View, ViewId, ViewState,
};
pub use widgets::{Editor, InputLine, ListBox, ListViewer, ListViewerState, ScrollBar, Scroller};
pub use widgets::{
    HistoryViewer, HistoryWindow, THistory, clear_history, history_add, history_count, history_str,
};
pub use window::{ScrollBarOptions, Window, WindowFlags, WindowPalette};
