//! `TDialog` — the modal dialog window (row 34).
//!
//! [`Dialog`] is the modality payoff: the first view you can
//! [`exec_view`](crate::app::Program::exec_view) and have it return
//! `cmOK`/`cmCancel`. It **embeds a [`Window`](crate::window::Window)** exactly as
//! `Window` embeds a [`Group`](crate::view::Group) — the D2 embed-and-delegate
//! pattern one level deeper. Almost the whole [`View`](crate::view::View) trait
//! delegates to the embedded window; the dialog's *own* behaviour is just three
//! things `TDialog` overrides:
//!
//! * **`handle_event`** — delegate to `Window::handle_event` first (faithful
//!   order), then the dialog keys (Esc → post `cmCancel`, Enter → broadcast
//!   `cmDefault`) and the modal-result commands (`cmOK`/`cmCancel`/`cmYes`/`cmNo`
//!   → `endModal` while `sfModal`).
//! * **`valid`** — `cmCancel` is *always* valid (cancelling a dialog can never be
//!   vetoed); every other command defers to the embedded group.
//! * **ctor field overrides** — `growMode = 0` (a dialog does not track owner
//!   resize), `flags = wfMove | wfClose` (no grow, no zoom), `palette = Gray`.
//!
//! ## Deviations in play
//! * **D2** embed-and-delegate (one level deeper than `Window`).
//! * **D9** the modal loop is the program's single [`pump_once`](crate::app::Program::pump_once)
//!   driven in a bounded top-level loop by
//!   [`Program::exec_view`](crate::app::Program::exec_view); `endModal` is the
//!   downward [`Deferred::EndModal`](crate::view::Deferred) request, not an
//!   up-pointer call.
//!
//! ## Deferred (row-34-adjacent, NOT built here — see the brief §6)
//! * **Gray multi-scheme theming.** `palette = Gray` is recorded on the window
//!   but the frame still renders the blue `Frame*` roles. Mapping `Gray`/`Cyan`
//!   to distinct theme roles is a separate cosmetic chunk with no functional
//!   dependency on the modal mechanism. `TODO(row 34 gray theming)`.
//! * **`getData`/`setData`/`dataSize` (D10).** No data-bearing controls exist
//!   until Batch B; the group gather/scatter walk has nothing to gather yet.
//! * **`message()`/`query` + the `cmCanCloseForm` veto.** `Dialog::valid` needs
//!   only [`Group::valid`](crate::view::Group); the return-consuming `message()`
//!   has no consumer at row 34 (it needs a validating control).

#[allow(clippy::module_inception)]
mod dialog;
mod filedlg;
mod msgbox;

pub use dialog::Dialog;
pub use filedlg::{
    CD_HELP_BUTTON, CD_NO_LOAD_DIR, CD_NORMAL, ChDirDialog, DirCollection, DirEntry, DirListBox,
    FA_DIREC, FD_CLEAR_BUTTON, FD_HELP_BUTTON, FD_NO_LOAD_DIR, FD_OK_BUTTON, FD_OPEN_BUTTON,
    FD_REPLACE_BUTTON, FileCollection, FileDialog, FileInfoPane, FileInputLine, FileList,
    SearchRec, search_rec_compare,
};
pub use msgbox::{MessageBoxButtons, MessageBoxKind};
pub(crate) use msgbox::{build_input_box, build_message_box};
