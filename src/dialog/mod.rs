//! The modal dialog window.
//!
//! [`Dialog`] is the view you run modally: hand it to
//! [`exec_view`](crate::app::Program::exec_view) and it returns the command that
//! closed it (`cmOK`/`cmCancel`/…). It **embeds a
//! [`Window`](crate::window::Window)** exactly as `Window` embeds a
//! [`Group`](crate::view::Group), and forwards almost the whole
//! [`View`](crate::view::View) trait to that window. The dialog's *own* behaviour
//! is just three things it overrides:
//!
//! * **`handle_event`** — let the window handle the event first, then the dialog
//!   keys (Esc → post `cmCancel`, Enter → broadcast `cmDefault`) and the
//!   modal-result commands (`cmOK`/`cmCancel`/`cmYes`/`cmNo` → end the modal loop
//!   while the dialog is modal).
//! * **`valid`** — `cmCancel` is *always* valid (cancelling a dialog can never be
//!   vetoed); every other command defers to the embedded group, so a control with
//!   a failing validator can keep the dialog open.
//! * **constructor field overrides** — no grow-with-owner, decoration flags
//!   `move | close` (no grow, no zoom), and the gray dialog color scheme.
//!
//! The modal loop is the program's single
//! [`pump_once`](crate::app::Program::pump_once) driven in a bounded top-level
//! loop by [`exec_view`](crate::app::Program::exec_view); ending the modal is a
//! downward [`Deferred::EndModal`](crate::view::Deferred) request routed to the
//! loop owner rather than an up-pointer call.
//!
//! Data-bearing dialogs work today: child controls (input lines, clusters, …)
//! carry typed values, optional [`Validator`](crate::validate::Validator)s gate
//! `cmReleasedFocus`/close, and the group's gather/scatter walk collects them — see
//! [`FileDialog`](crate::dialog::FileDialog) for a worked example.
//!
//! # Turbo Vision heritage
//! Ports `TDialog` (`tdialog.cpp`/`dialogs.h`). C++ `TDialog : TWindow`
//! inheritance becomes embed-and-delegate composition (deviation D2) — the dialog
//! holds a `Window` and forwards to it; the single modal loop and the downward
//! end-modal request are deviation D9.

mod colorpick;
#[allow(clippy::module_inception)]
mod dialog;
mod filedlg;
mod msgbox;
mod theme_editor;

pub use colorpick::{ColorPicker, Tab};
pub use dialog::Dialog;
pub use filedlg::{
    CD_HELP_BUTTON, CD_NO_LOAD_DIR, CD_NORMAL, ChDirDialog, DirCollection, DirEntry, DirListBox,
    FA_DIREC, FD_CLEAR_BUTTON, FD_HELP_BUTTON, FD_NO_LOAD_DIR, FD_OK_BUTTON, FD_OPEN_BUTTON,
    FD_REPLACE_BUTTON, FileCollection, FileDialog, FileInfoPane, FileInputLine, FileList,
    SearchRec, search_rec_compare,
};
pub use msgbox::{MessageBoxButtons, MessageBoxKind};
pub(crate) use msgbox::{build_input_box, build_message_box};
pub(crate) use theme_editor::ThemeEditorBody;
