//! Downward draw and event/update contexts.
//!
//! There are no up-pointers from a view to its parent: instead a parent passes a
//! context *down* carrying everything a child would otherwise reach upward for.
//! There are two:
//!
//! * [`DrawCtx`] — the clipped, themed writer a view paints through during
//!   `draw()`. It works in *view-local* coordinates; the ctx translates them to
//!   absolute screen coordinates and clips. Its write ops are built on top of the
//!   [`Buffer`] and the [`text`](crate::text) primitives — never re-deriving
//!   wide-char logic.
//! * [`Context`] — the event/update context handlers and `handle_event` reach
//!   for. It exposes the `ctx.*` call surface (post / broadcast / timer
//!   scheduling / deferred capture push). It is built over loop-owned state as
//!   **distinct `&mut` fields** so the event loop can take disjoint-field
//!   borrows; the fields are deliberately not hidden behind one getter.

use crate::capture::CaptureHandler;
use crate::color::{Color, Style};
use crate::command::{Command, CommandSet};
use crate::event::Event;
use crate::screen::Buffer;
use crate::theme::{Glyphs, Role, Theme};
use crate::timer::{TimerId, TimerQueue};
use crate::view::geometry::{Point, Rect};
use crate::view::id::ViewId;
use crate::view::view::{Phase, StateFlag};
use std::collections::VecDeque;
use std::time::Duration;
use unicode_width::UnicodeWidthChar;

// ---------------------------------------------------------------------------
// Deferred — an effect on loop-owned state requested through Context
// ---------------------------------------------------------------------------

/// One operation on a splitter's keyboard-resize session, brokered by id from
/// the window's resize capture (which knows the splitter only by [`ViewId`]).
///
/// Used as the payload of [`Deferred::SplitterDivider`] (the D3 sibling-broker
/// for the splitter resize path, exactly like
/// [`Deferred::SyncScrollerDelta`](Deferred::SyncScrollerDelta) for the scroller).
#[derive(Debug, Clone)]
pub enum DividerOp {
    /// Set (or clear) the active-target divider highlight.
    SetActive(Option<usize>),
    /// Move divider `index` by `delta` cells along the split axis.
    Nudge { index: usize, delta: i32 },
    /// End the session; `commit=false` restores the snapshotted weights.
    EndSession { commit: bool },
}

/// An effect on loop-owned state that a downward-borrowed view / capture handler
/// cannot perform inline. During dispatch the view tree is a live `&mut`
/// borrow stack (root → desktop → window → frame): a view cannot reach *up* or
/// *sideways* — every ancestor is already `&mut`-borrowed above it, and a fresh
/// `root.find_mut(id)` would alias that borrow. Nor does a view hold the program's
/// capture stack or command set. So any such effect is **requested** through
/// [`Context`] (which pushes a variant here) and **applied** by the loop after the
/// dispatch unwinds and the root is free again.
///
/// One queue, drained once per pump in **insertion order**. The variants fall into
/// four disjoint families by the loop-owned state they touch — capture stack,
/// command set, view tree, loop state (`end_state`) — so cross-family apply order
/// never affects the result;
/// same-family items keep their relative order.
pub enum Deferred {
    /// Push a capture handler onto the program's capture stack. Applied *after* the
    /// current dispatch, so the pushed handler sees the *next* event, never the
    /// current one.
    PushCapture(Box<dyn CaptureHandler>),
    /// Enable a command in the program's command set.
    EnableCommand(Command),
    /// Disable a command in the program's command set.
    DisableCommand(Command),
    /// Apply new bounds to the view named by `ViewId` (drag move/grow). No ctx
    /// needed at apply time (`change_bounds` takes none).
    ChangeBounds(ViewId, Rect),
    /// Flip a propagating state flag on the view (e.g. clear `dragging` at drag end).
    SetState(ViewId, StateFlag, bool),
    /// Remove the view from whichever group owns it (the close command).
    Close(ViewId),
    /// Focus (select) the view named by `ViewId` within its owning group (a label
    /// focusing its linked control). The pump resolves it via
    /// [`View::focus_descendant`](crate::view::View::focus_descendant), which walks
    /// to the owning group and runs `focus_child` (the selectable gate lives in
    /// that group walk, not at the request site). A view (the label) holds only the
    /// link's [`ViewId`], so it cannot select a sibling inline.
    FocusById(ViewId),
    /// Request the (modal) loop end with `command`. The pump applies it by setting
    /// `Program::end_state`; the nested `exec_view` loop then observes it. The
    /// downward replacement for a view ending its own modal loop.
    ///
    /// This touches **loop state** (`end_state`) — a fourth disjoint target
    /// alongside the capture stack / command set / view tree — so the
    /// insertion-order drain stays order-equivalent: no dispatch co-queues an
    /// `EndModal` with an effect on the *same* state, and cross-family order never
    /// affects the result.
    EndModal(Command),

    // -- the scroller cross-view scrollbar broker ---------------
    //
    // All three touch the **view tree** family (same as `ChangeBounds`/`SetState`/
    // `Close`/`FocusById`), so the insertion-order drain stays order-equivalent:
    // no single dispatch co-queues two ops on the *same* scrollbar/scroller in a
    // conflicting order. They exist because a leaf view (the scroller) holds only
    // `&mut Context` and so can neither **read** nor
    // **mutate** its window-frame sibling scrollbars; the pump — which owns the
    // whole tree — is the cross-view broker, performing every read/write at
    // deferred-apply time via `group.find_mut(id)`.
    /// **Read direction**: resolve the `h`/`v` scrollbars, read each `value` (via
    /// [`View::value`](crate::view::View::value) →
    /// [`FieldValue::Int`](crate::data::FieldValue::Int)), and push the resulting
    /// delta into `scroller` (the pump downcasts it to `Scroller` and calls
    /// `apply_delta`, which adjusts the cursor and stores the new delta). The
    /// scroller requests this from its event handler when a scrollbar-changed
    /// broadcast names one of its bars as `source`.
    SyncScrollerDelta {
        /// The scroller whose `delta`/`cursor` to update.
        scroller: ViewId,
        /// The horizontal scrollbar to read `value` from (`None` = no h bar → 0).
        h: Option<ViewId>,
        /// The vertical scrollbar to read `value` from (`None` = no v bar → 0).
        v: Option<ViewId>,
    },
    /// **Write direction**: update a scrollbar's value and/or range/step. The pump
    /// resolves `id`, downcasts to `ScrollBar`, fills each `None` field from the
    /// bar's **live** value (preserve-where-`None`), then calls `set_params` —
    /// which clamps and may re-broadcast that its value changed. One flexible
    /// variant serves scrollers, list viewers, and editors.
    ScrollBarSetParams {
        /// The scrollbar to update.
        id: ViewId,
        /// New value, or `None` to preserve the bar's live `value`.
        value: Option<i32>,
        /// New range minimum, or `None` to preserve `min_value`.
        min: Option<i32>,
        /// New range maximum, or `None` to preserve `max_value`.
        max: Option<i32>,
        /// New page step, or `None` to preserve `page_step`.
        page_step: Option<i32>,
        /// New arrow step, or `None` to preserve `arrow_step`.
        arrow_step: Option<i32>,
    },
    /// **Visibility direction**: show/hide a scroller's scrollbar. The pump
    /// resolves `id` and sets `state.state.visible` (no downcast — `state_mut` is
    /// on the trait; the painter skips `!visible` children). There is no
    /// propagating `StateFlag::Visible` (no occlusion tracking — the whole tree is
    /// redrawn each frame, so visibility carries no side effects), so it is set
    /// directly on the [`ViewState`](crate::view::ViewState).
    SetVisible(ViewId, bool),

    // -- the splitter keyboard-resize broker (D3 sibling-broker) -------
    /// Apply a [`DividerOp`] to the splitter named by `splitter`. The pump
    /// resolves it via `group.find_mut(splitter).as_any_mut()` → `Splitter`.
    /// Touches the **view-tree** deferred family (same as the scroller ops), so
    /// the insertion-order drain stays order-equivalent: a single dispatch never
    /// co-queues conflicting ops on the same splitter.
    SplitterDivider {
        /// The splitter whose divider session to update.
        splitter: ViewId,
        /// The operation to apply to the splitter's keyboard-resize session.
        op: DividerOp,
    },

    // -- the list-viewer cross-view scrollbar read-sync ----------
    /// **Read direction for a list viewer**: on a scrollbar-changed broadcast,
    /// resolve the `h`/`v` scrollbars, read each `value`
    /// (via [`View::value`](crate::view::View::value) →
    /// [`FieldValue::Int`](crate::data::FieldValue::Int)), then call
    /// [`View::apply_list_scroll`](crate::view::View::apply_list_scroll) on the
    /// `list` view (the trait method — NOT a downcast: list viewers are a shared
    /// trait, so a `dyn View →` concrete downcast cannot work, unlike the scroller).
    ///
    /// **Termination (the centerpiece property):** unlike
    /// [`SyncScrollerDelta`](Self::SyncScrollerDelta), this read-sync **writes
    /// back** — `apply_list_scroll`'s item-focus call requests a value update on
    /// the v-bar (another [`ScrollBarSetParams`](Self::ScrollBarSetParams)). That
    /// terminates because
    /// [`ScrollBar::set_params`](crate::widgets::ScrollBar::set_params) is
    /// **change-guarded**: it re-broadcasts only on an actual value change, so
    /// writing back the already-current value is a silent no-op (steady state:
    /// quiescent; after a clamp: one extra round then quiescent).
    ///
    /// Touches the **view-tree** family (same as the scroller broker ops), so the
    /// insertion-order drain stays order-equivalent.
    SyncListViewer {
        /// The list view whose `focused`/`top_item`/`indent` to update.
        list: ViewId,
        /// The horizontal scrollbar to read `value` from (`None` = no h bar).
        h: Option<ViewId>,
        /// The vertical scrollbar to read `value` from (`None` = no v bar).
        v: Option<ViewId>,
    },

    // -- the outline-viewer scrollbar read-sync ------------------
    /// **Read-direction sync for an outline viewer** (on a scrollbar-changed
    /// broadcast). The pump resolves both bars, reads each `value` (via
    /// [`View::value`] → [`FieldValue::Int`](crate::data::FieldValue::Int)), and
    /// writes the resulting `(dx, dy)` into `viewer`'s `delta` (the pump downcasts
    /// it to `Outline` and calls `apply_delta`). Like
    /// [`SyncScrollerDelta`](Self::SyncScrollerDelta) this is **read-only** — it
    /// writes nothing back to the bars, so it terminates with no change-guard
    /// needed (unlike [`SyncListViewer`](Self::SyncListViewer)).
    ///
    /// Touches the **view-tree** family (same as the scroller/list broker ops), so
    /// the insertion-order drain stays order-equivalent.
    SyncOutlineViewerDelta {
        /// The outline viewer whose `delta` to update.
        viewer: ViewId,
        /// The horizontal scrollbar to read `value` from (`None` = no h bar → 0).
        h: Option<ViewId>,
        /// The vertical scrollbar to read `value` from (`None` = no v bar → 0).
        v: Option<ViewId>,
    },

    // -- the menu command-graying broker -------------------------
    /// **Command-graying broker for a menu view** (triggered by a
    /// command-set-changed broadcast). Resolve the menu view by `id` and call
    /// [`View::update_menu_commands`](crate::view::View::update_menu_commands) with
    /// the pump's **live** [`CommandSet`](crate::command::CommandSet), which
    /// re-grays the menu tree (an item is disabled iff its command is disabled,
    /// recursing submenus).
    ///
    /// A broker — **not** a `&CommandSet` read-accessor on [`Context`] — because
    /// the command set lives on `Program` and the apply-phase `Context` is alive
    /// across a loop whose `EnableCommand`/`DisableCommand` arms mutate
    /// `disabled_commands` (`&mut`); a `&CommandSet` on `Context` would alias
    /// that borrow. The view (a child) cannot read the command set inline, so
    /// it requests this by its own id and the pump calls back at apply time,
    /// exactly like [`SyncListViewer`](Self::SyncListViewer) + `apply_list_scroll`.
    /// (For a plain *read* a view does NOT need this broker:
    /// [`Context::command_enabled`] answers from an owned per-pump **snapshot**
    /// of the disabled set — a clone, not a borrow, so the aliasing problem never
    /// arises. The broker remains the write-back path for regraying caches.)
    ///
    /// Touches the **view-tree** family (same as the scroller/list broker ops), so
    /// the insertion-order drain stays order-equivalent.
    UpdateMenu(ViewId),

    // -- the internal-clipboard editor broker ------------------------
    //
    // All three variants touch the **view-tree** family (same as `ChangeBounds` /
    // `SetState` / `Close` / `FocusById`), so the insertion-order drain stays
    // order-equivalent: no single dispatch co-queues two ops on the same clipboard
    // editor in a conflicting order. They exist because a leaf editor holds only
    // `&mut Context` and so cannot read or mutate the clipboard editor (a
    // sibling / other window's child) inline; the pump — which owns the whole
    // tree — is the cross-view broker.
    /// **Register an editor as the internal clipboard.** The pump stores the id on
    /// `Program::clipboard_editor_id`, marks the editor `is_clipboard = true`, and
    /// sets the hosting edit window's title to "Clipboard". Touches the
    /// **view-tree** family + `clipboard_editor_id` (loop state), so the
    /// insertion-order drain stays order-equivalent.
    RegisterClipboardEditor {
        /// The editor to register as the internal clipboard.
        editor_id: ViewId,
        /// The edit window whose title to set to "Clipboard".
        window_id: ViewId,
    },
    /// **Copy source bytes into the clipboard editor** (a copy from another
    /// editor). The pump finds the clipboard editor and calls `insert_from(&data)`
    /// (which selects the inserted text for the clipboard editor). Touches the
    /// **view-tree** family.
    ClipboardEditorReceive {
        /// The clipboard editor that receives the data.
        clipboard_id: ViewId,
        /// The raw bytes to insert (the source editor's selection snapshot).
        data: Vec<u8>,
    },
    /// **Paste the clipboard editor's selection into the destination editor.** The
    /// pump reads the clipboard editor's selection bytes (step 1), then calls
    /// `insert_from(&data)` on the dest editor (step 2, two separate `find_mut`
    /// calls). Touches the **view-tree** family.
    ClipboardEditorPaste {
        /// The destination editor to paste into.
        dest_id: ViewId,
        /// The clipboard editor whose selection to read.
        clipboard_id: ViewId,
    },

    // -- the menu modal layer (MenuSession) ------------
    /// **Open a menu box** — the deferred realization of opening a submenu. The
    /// [`MenuSession`](crate::menu::MenuSession) capture handler **pre-mints** `id`
    /// from [`ViewId::next`](crate::view::ViewId) so it already knows the box id
    /// with no insert-time callback; the pump builds a
    /// [`MenuBox`](crate::menu::MenuBox) from `menu` over `bounds` and
    /// [`Group::insert_with_id`](crate::view::Group::insert_with_id)s it into the
    /// root group, stamping that id. **No focus move** — the box is never current
    /// (the session owns every event). `menu` is a clone of the submenu subtree
    /// (clone-at-open is fine — an open menu receives no broadcasts, so its
    /// disabled state is frozen for the box's lifetime).
    ///
    /// Touches the **view-tree** family (same as `Close`/the broker ops), so the
    /// insertion-order drain stays order-equivalent. The activation site queues
    /// the [`PushCapture`](Self::PushCapture) of the session AND the first
    /// `OpenMenuBox` in the same batch (no dead first event).
    OpenMenuBox {
        /// The pre-minted id the box will be stamped with.
        id: ViewId,
        /// The (cloned) submenu subtree the box presents.
        menu: crate::menu::Menu,
        /// The box bounds in the root group's frame.
        bounds: Rect,
    },
    /// **Set a menu view's highlight cache** (the highlighted item index). The
    /// pump resolves `id` and calls
    /// [`View::set_menu_current`](crate::view::View::set_menu_current) (a trait
    /// method, mirroring the `update_menu_commands` broker — no downcast). This is
    /// the write-only display cache the bar/box `draw` reads to pick the selected
    /// colour; the [`MenuSession`](crate::menu::MenuSession) owns the authoritative
    /// highlight and pushes it here whenever navigation moves it.
    ///
    /// Touches the **view-tree** family, so the insertion-order drain stays
    /// order-equivalent.
    SetMenuCurrent(ViewId, Option<usize>),

    // -- the history view-triggered async-modal seam -----------
    /// **View-triggered modal open** (the history drop-down). Built at apply time
    /// because the trigger view holds only the link's id: the pump reads the link,
    /// records history, builds the history window, and stashes it into
    /// `Program::pending_modal` — it does **not** call `exec_view` here (the apply
    /// phase is inside the `pump_once` destructure; a view cannot call `exec_view`,
    /// which is top-level only). The OUTER driver loop runs `exec_view` at top
    /// level after `pump_once` returns.
    ///
    /// Touches the **view-tree** family + **loop state** (`pending_modal`), like
    /// the other tree ops + `EndModal`, so the insertion-order drain stays
    /// order-equivalent (no dispatch co-queues a conflicting op on the same state).
    OpenHistory {
        /// The linked input line whose text/bounds/focus drive the open + flowback.
        link: ViewId,
        /// The history channel id.
        history_id: u8,
        /// True for the keyboard trigger (gate on the link being focused); false
        /// for the mouse trigger.
        require_focus: bool,
    },
    /// **Record the link's current text into history** (on the link losing focus or
    /// an explicit record request): resolve the link, read its text,
    /// `history_add(id, text)`. Touches no loop-owned state beyond the read of the
    /// view tree (a pure side effect on the process-global history store), so it is
    /// order-equivalent with every other family.
    RecordHistory { link: ViewId, history_id: u8 },

    // -- the editor cross-view brokers ---------------------------
    /// **Read direction for an editor** (on a scrollbar-changed broadcast).
    /// Resolve the `h`/`v` scrollbars, read each `value`
    /// (via [`View::value`](crate::view::View::value)), downcast `editor` to
    /// [`Editor`](crate::widgets::Editor) and call `apply_scroll_delta(dx, dy)`
    /// (which updates the delta and redraws only on a change). The editor is **not**
    /// a `Scroller`, so it cannot reuse
    /// [`SyncScrollerDelta`](Self::SyncScrollerDelta). Touches the **view-tree**
    /// family, so the insertion-order drain stays order-equivalent.
    SyncEditorDelta {
        /// The editor whose `delta` to update.
        editor: ViewId,
        /// The horizontal scrollbar to read `value` from (`None` = no h bar).
        h: Option<ViewId>,
        /// The vertical scrollbar to read `value` from (`None` = no v bar).
        v: Option<ViewId>,
    },
    /// **Indicator write**: update an editor's position/modified indicator.
    /// Resolve `indicator`, downcast to [`Indicator`](crate::widgets::Indicator),
    /// and call `set_value(location, modified)`. The editor (a leaf) cannot mutate
    /// its sibling indicator inline. Touches the **view-tree** family.
    IndicatorSetValue {
        /// The indicator to update.
        indicator: ViewId,
        /// The cursor position to display.
        location: Point,
        /// Whether the buffer has unsaved changes.
        modified: bool,
    },
    /// **Copy text to the system clipboard.** The pump calls
    /// `renderer.backend_mut().set_clipboard(&s)`. Touches the backend only, so it
    /// is order-equivalent with every family.
    SetClipboard(String),
    /// **Paste from the system clipboard into an editor.** The pump reads
    /// `renderer.backend_mut().get_clipboard()`, downcasts `editor` to
    /// [`Editor`](crate::widgets::Editor), and inserts the text. Touches the
    /// **view-tree** family + the backend.
    EditorPaste(ViewId),
    /// **Paste from the system clipboard into an input line.** The pump reads
    /// `renderer.backend_mut().get_clipboard()`, downcasts the view named by `id`
    /// to [`InputLine`](crate::widgets::InputLine) via `as_any_mut`, and calls
    /// [`paste_text`](crate::widgets::InputLine::paste_text) — which inserts at
    /// the cursor, replacing any selection and clamping to `max_len`. Touches
    /// the **view-tree** family + the backend (same as
    /// [`EditorPaste`](Self::EditorPaste)).
    InputLinePaste(ViewId),

    // -- the payload-carrying-broadcast (file-focused) broker --
    /// **Resolve a file-focused broadcast's directory-entry payload** (the file
    /// input line / info pane consumers). rstv's
    /// [`Event::Broadcast`](crate::event::Event::Broadcast) is payload-less
    /// (`source` is the resolvable subject, NOT a value carrier), so this is the
    /// resolve-by-source broker — the same shape as
    /// [`SyncListViewer`](Self::SyncListViewer)'s read+write, but reading a
    /// directory entry rather than a scrollbar value.
    ///
    /// The producer ([`FileList`](crate::dialog::FileList)) broadcasts
    /// `FILE_FOCUSED { source = its own id }`; the consumer (a leaf holding only
    /// `&mut Context`, so it cannot read its `FileList` sibling) filters on the
    /// command and requests this. The pump resolves `source` (downcast `FileList`,
    /// read its [`focused_rec`](crate::dialog::FileList::focused_rec)), then writes
    /// the record into `subscriber` (downcast
    /// [`FileInputLine`](crate::dialog::FileInputLine), call `on_file_focused`).
    /// Two separate `find_mut` calls keep only one `&mut` live at a time (exactly
    /// like [`SyncScrollerDelta`](Self::SyncScrollerDelta)'s read-then-write).
    ///
    /// Touches the **view-tree** family (same as the scroller/list broker ops), so
    /// the insertion-order drain stays order-equivalent.
    ResolveFocusedFile {
        /// The consumer view to write the focused record into (the file input line).
        subscriber: ViewId,
        /// The producer view (the file list) whose focused entry to read.
        source: ViewId,
    },

    // -- the directory-list → change-dir button default broker ---
    /// **Make a sibling [`Button`](crate::widgets::Button) the default** on a
    /// directory-list focus change. The directory list is a leaf holding only
    /// `&mut Context`, so it cannot reach its sibling button inline; it queues this
    /// and the pump resolves `button`, downcasts to
    /// [`Button`](crate::widgets::Button), and calls
    /// [`make_default`](crate::widgets::Button::make_default) (which re-broadcasts a
    /// grab/release-default notification so the real default button relinquishes /
    /// retakes the look — that re-broadcast settling on the next pump is expected,
    /// like the other write-back brokers).
    ///
    /// Touches the **view-tree** family (same as the scroller/list broker ops), so
    /// the insertion-order drain stays order-equivalent.
    MakeButtonDefault {
        /// The button to make (or un-make) the default.
        button: ViewId,
        /// True when the dir list gained focus (grab the default), false when it
        /// lost focus (release).
        enable: bool,
    },

    // -- the async-modal-from-a-view seam (messageBox from valid()) -----------
    /// **View-triggered modal `messageBox`** (the async-modal-from-a-view seam —
    /// `docs/design/async-modal-from-view.md`). A downward-borrowed `&mut View`
    /// (a validator `error`, the `FileEditor` modified-save prompt) cannot run a
    /// nested modal inline (it holds only `&mut Context`), so it requests one here.
    /// The pump builds the centered box, stashes it into `Program::pending_modal`
    /// with a [`RouteModalAnswer`](crate::app) completion, and the outer
    /// `pump_and_drive` runs it at top level (mirroring [`OpenHistory`](Self::OpenHistory)).
    ///
    /// Touches the **view-tree** family + **loop state** (`pending_modal`), like
    /// `OpenHistory` + `EndModal`, so the insertion-order drain stays
    /// order-equivalent.
    OpenMessageBox {
        /// The message text (already-formatted, exact-string).
        text: String,
        /// Picks the box title (Error / Information / …).
        kind: crate::dialog::MessageBoxKind,
        /// Which buttons to show (OK-only for informational; Yes/No/Cancel for a prompt).
        buttons: crate::dialog::MessageBoxButtons,
        /// Route the chosen [`Command`](crate::command::Command) to this view (via
        /// [`View::set_modal_answer`](crate::view::View::set_modal_answer)) after the
        /// box closes. `None` = informational (OK-only) — no routing.
        answer_to: Option<ViewId>,
        /// After routing the answer, re-post this focused command so the original
        /// action (e.g. [`Command::CLOSE`](crate::command::Command::CLOSE)) re-runs
        /// `valid()` with the cached answer. `None`
        /// for informational boxes and for the inline modal-close path (which
        /// re-validates inline).
        then_command: Option<crate::command::Command>,
    },

    // -- saveAs: the view-triggered FileDialog seam ---------------------------
    /// Request the pump to open a [`FileDialog`](crate::dialog::FileDialog) for the
    /// given editor (a view-triggered async-modal, the `HistoryPick`/`OpenMessageBox`
    /// shape). A [`FileEditor`](crate::widgets::FileEditor) leaf holds only `&mut
    /// Context`, so it cannot run the nested file-picker modal inline; it requests
    /// it here. The pump builds the `FileDialog` ("Save file as", FD_OK_BUTTON) and
    /// stashes it into `Program::pending_modal` with a `SaveAsPick { editor_id }`
    /// completion, which the outer `pump_and_drive` runs at top level. On accept the
    /// completion sets the chosen filename on the editor and re-injects
    /// [`Command::SAVE`](crate::command::Command::SAVE).
    OpenSaveAsDialog {
        /// The [`FileEditor`](crate::widgets::FileEditor) to save the picked name to.
        editor_id: ViewId,
    },

    // -- find/replace dialogs (the find/replace editor seam) --------------
    /// Request the pump to open the Find dialog
    /// ([`Command::FIND`](crate::command::Command::FIND)) for the given editor.
    /// An [`Editor`](crate::widgets::Editor) leaf holds only `&mut Context`, so
    /// it cannot exec the dialog inline; it requests it here. The pump builds
    /// the dialog and stashes it into `Program::pending_modal` with a
    /// `FindPick { editor_id }` completion, which the outer `pump_and_drive`
    /// runs at top level. On accept the completion reads back the search string
    /// plus options and re-injects
    /// [`Command::SEARCH_AGAIN`](crate::command::Command::SEARCH_AGAIN).
    OpenFindDialog {
        /// The [`Editor`](crate::widgets::Editor) to update.
        editor_id: ViewId,
    },

    /// Request the pump to open the Replace dialog
    /// ([`Command::REPLACE`](crate::command::Command::REPLACE)) for the given
    /// editor. Mirror of [`OpenFindDialog`](Self::OpenFindDialog) but for the
    /// find+replace variant. On accept the completion sets `EF_DO_REPLACE` and
    /// re-injects [`Command::SEARCH_AGAIN`](crate::command::Command::SEARCH_AGAIN).
    OpenReplaceDialog {
        /// The [`Editor`](crate::widgets::Editor) to update.
        editor_id: ViewId,
    },

    // -- the per-role color-picker seam (theme editor) -----------
    /// Request the pump to open a [`ColorPicker`](crate::dialog::ColorPicker)
    /// dialog for a specific theme role component. A
    /// [`ThemeEditorBody`](crate::dialog::ThemeEditorBody) leaf holds only
    /// `&mut Context` and cannot exec the dialog inline; it requests it here.
    /// The pump builds a 60×23 "Select Color" dialog and sets `pending_modal`
    /// with a `ThemeColorPick { editor_id, picker, role, fg }` completion,
    /// which `pump_and_drive` runs at top level. On OK the completion updates
    /// the `ThemeEditorBody`'s working theme.
    OpenColorDialogForRole {
        /// `ViewId` of the `ThemeEditorBody` to update on completion.
        editor_id: crate::view::ViewId,
        /// The role whose style is being edited.
        role: crate::theme::Role,
        /// `true` = editing foreground color; `false` = editing background color.
        fg: bool,
        /// Current color to seed the picker with.
        current: crate::color::Color,
    },

    // -- the mouse hold-tracking router (MouseTrackCapture seam) -------
    /// **Deliver a localized mouse event to the tracked view** while a mouse
    /// button is held. Posted only by
    /// [`MouseTrackCapture`](crate::capture::MouseTrackCapture), for each masked
    /// `MouseMove` / `MouseAuto` / `MouseWheel` event and for the terminating
    /// `MouseUp`. The pump resolves `view` via `group.find_mut` and calls
    /// `handle_event(&mut event, …)` directly (the apply-time analogue of the
    /// outside-modal redirect): the widget's `MouseMove`/`MouseAuto`/`MouseUp`
    /// arms do the per-event work, so no widget downcast is needed here (decisive
    /// for trait-object viewers like [`ListViewer`](crate::widgets::ListViewer) /
    /// [`Outline`](crate::widgets::Outline)). `event` is already **view-local**
    /// (the capture subtracted the origin cached at push time). Touches the
    /// **view-tree** family, so the insertion-order drain stays order-equivalent.
    ///
    /// Direct delivery deliberately bypasses the `Group::wants` event-mask gate:
    /// the hold reads events straight off the queue, not through the tracked
    /// view's [`event_mask`](crate::view::ViewState::event_mask) — faithfully
    /// reproducing the original blocking hold loop, which read input directly.
    MouseTrack {
        /// The view being mouse-tracked (the one that pushed the capture).
        view: ViewId,
        /// The localized event to deliver (`MouseMove`/`MouseAuto`/wheel
        /// `MouseDown`/`MouseUp`, position already view-local).
        event: Event,
    },

    // -- the PageStack↔TabBar read-sync broker --------------------------
    /// **Read-broker for a [`PageStack`](crate::widgets::PageStack)**: on a
    /// `TAB_BAR_CHANGED` broadcast, the pump resolves `tab_bar`, reads its
    /// `value()` (→ `FieldValue::Int` index), downcasts `page_stack` to
    /// `PageStack`, and calls `set_active(index, &mut ctx)`. Mirrors
    /// [`SyncScrollerDelta`](Deferred::SyncScrollerDelta).
    ///
    /// Touches the **view-tree** family (same as the scroller broker ops), so
    /// the insertion-order drain stays order-equivalent.
    PageStackSync {
        /// The [`PageStack`](crate::widgets::PageStack) whose active page to update.
        page_stack: ViewId,
        /// The [`TabBar`](crate::widgets::TabBar) whose `value()` to read.
        tab_bar: ViewId,
    },
}

// ---------------------------------------------------------------------------
// DrawCtx — the downward draw context
// ---------------------------------------------------------------------------

/// The drop-shadow offset: 2 columns right, 1 row down.
///
/// # Turbo Vision heritage
/// Ports `shadowSize` (`tview.cpp:35`).
pub const SHADOW_SIZE: Point = Point::new(2, 1);

/// True iff `bg` counts as black for the shadow transform — i.e. its BIOS index
/// reduces to 0, with the default color treated as black. The xterm-256/RGB
/// quantization ladder lives in the backend, so this is a documented
/// simplification: only the exact black values (`Indexed(0)`/`Indexed(16)`,
/// `Rgb(0,0,0)`) count as black; near-black values the ladder would quantize to
/// BIOS 0 do not.
fn bg_is_black(bg: Color) -> bool {
    match bg {
        Color::Default => true, // the default background reduces to black (see fn-level doc)
        Color::Bios(b) => b & 0xF == 0,
        Color::Indexed(i) => i == 0 || i == 16,
        Color::Rgb(r, g, b) => (r, g, b) == (0, 0, 0),
    }
}

/// The clipped, themed writer every view paints through.
///
/// All public write methods take **view-local** coordinates: `(0, 0)` is the
/// view's own top-left. The ctx adds [`origin`](Self::origin) to translate into
/// absolute screen columns/rows, and clips every write to [`clip`](Self::clip).
/// The clip is stored as an **absolute** rect already intersected with the
/// buffer bounds at construction, so a write can never index the buffer out of
/// range.
///
/// # Turbo Vision heritage
/// The successor to `TDrawBuffer` plus the owner-relative coordinate math in
/// `TView::writeLine`/`writeBuf` (`tvwrite.cpp`). A view receives this context
/// downward instead of reaching up through an owner pointer (deviation D3).
pub struct DrawCtx<'a> {
    buffer: &'a mut Buffer,
    /// Absolute clip rect, already intersected with the buffer's `(0,0,w,h)`.
    clip: Rect,
    /// View-local `(0, 0)` maps to this absolute screen position.
    origin: Point,
    theme: &'a Theme,
}

impl<'a> DrawCtx<'a> {
    /// Build a draw context.
    ///
    /// `clip` is intersected with the buffer's bounds (`(0, 0, width, height)`)
    /// at construction and stored absolute, so the write methods can never index
    /// out of bounds.
    pub fn new(buffer: &'a mut Buffer, theme: &'a Theme, clip: Rect, origin: Point) -> Self {
        let bounds = Rect::new(0, 0, buffer.width() as i32, buffer.height() as i32);
        let mut clip = clip;
        clip.intersect(&bounds);
        DrawCtx {
            buffer,
            clip,
            origin,
            theme,
        }
    }

    /// The [`Style`] for `role` from the active theme.
    pub fn style(&self, role: Role) -> Style {
        self.theme.style(role)
    }

    /// The theme's glyph holder.
    pub fn glyphs(&self) -> &Glyphs {
        self.theme.glyphs()
    }

    /// The absolute clip rect (already intersected with the buffer bounds).
    pub fn clip(&self) -> Rect {
        self.clip
    }

    /// The absolute screen position that view-local `(0, 0)` maps to.
    pub fn origin(&self) -> Point {
        self.origin
    }

    /// Write one cell at view-local `(x, y)` with `style`.
    ///
    /// A double-width `ch` sets the lead `wide` and the next cell `wide_trail`,
    /// but only if both fall inside the clip; if the trail would fall outside,
    /// a space is written instead. Anything fully outside the clip is dropped
    /// (never panics).
    pub fn put_char(&mut self, x: i32, y: i32, ch: char, style: Style) {
        if self.clip.is_empty() {
            return;
        }
        let ax = x + self.origin.x;
        let ay = y + self.origin.y;
        if ay < self.clip.a.y || ay >= self.clip.b.y {
            return;
        }
        if ax < self.clip.a.x || ax >= self.clip.b.x {
            return;
        }
        let wide = UnicodeWidthChar::width(ch).unwrap_or(1) > 1;
        let row = self.buffer.row_mut(ay as u16);
        let i = ax as usize;
        if wide && ax + 1 < self.clip.b.x {
            // Room for both halves inside the clip.
            let mut buf = [0u8; 4];
            row[i].set_str(ch.encode_utf8(&mut buf), true);
            row[i].set_style(style);
            row[i + 1].set_wide_trail();
            row[i + 1].set_style(style);
        } else if wide {
            // Trail would fall outside the clip — degrade to a space.
            row[i].set_char(' ');
            row[i].set_style(style);
        } else {
            row[i].set_char(ch);
            row[i].set_style(style);
        }
    }

    /// Write `s` at view-local `(x, y)` with a fixed `style`, width-aware and
    /// clipped. Returns the number of columns actually written.
    ///
    /// Delegates the wide-char and edge-straddle logic to [`text::draw_str`],
    /// exactly as `DrawBuffer::move_str_part` does — the string is written into
    /// the clipped sub-slice of the target buffer row, with `indent` /
    /// `text_indent` chosen so a glyph straddling either clip edge degrades the
    /// same way `move_str_part` already handles it.
    pub fn put_str(&mut self, x: i32, y: i32, s: &str, style: Style) -> i32 {
        if self.clip.is_empty() {
            return 0;
        }
        let ay = y + self.origin.y;
        if ay < self.clip.a.y || ay >= self.clip.b.y {
            return 0;
        }
        let ax = x + self.origin.x;
        // The writable window for this row is the clip's column span.
        let lo = self.clip.a.x as usize;
        let hi = self.clip.b.x as usize; // > lo, since clip is non-empty
        let row = &mut self.buffer.row_mut(ay as u16)[lo..hi];

        let (indent, text_indent) = if ax >= self.clip.a.x {
            // String starts at or after the clip left edge: indent into the
            // sub-slice; right-edge truncation falls out of `draw_str` running
            // out of cells.
            ((ax - self.clip.a.x) as usize, 0)
        } else {
            // String starts left of the clip: skip the off-screen columns via
            // text_indent (this is move_str_part's left-edge straddle path).
            (0, self.clip.a.x - ax)
        };

        crate::text::draw_str(row, indent, s, text_indent, style) as i32
    }

    /// Write `s` at view-local `(x, y)` with a fixed `style`, starting from
    /// display column `text_indent` of `s` (skipping that many leading columns) —
    /// used to render a horizontally-scrolled input field. Width-aware and clipped
    /// exactly like [`put_str`](Self::put_str). Returns columns written.
    ///
    /// A glyph straddling the `text_indent` boundary degrades to a space (the
    /// `move_str_part` left-edge straddle), via [`text::draw_str`].
    pub fn put_str_part(&mut self, x: i32, y: i32, s: &str, text_indent: i32, style: Style) -> i32 {
        if self.clip.is_empty() {
            return 0;
        }
        let ay = y + self.origin.y;
        if ay < self.clip.a.y || ay >= self.clip.b.y {
            return 0;
        }
        let ax = x + self.origin.x;
        let lo = self.clip.a.x as usize;
        let hi = self.clip.b.x as usize;
        let row = &mut self.buffer.row_mut(ay as u16)[lo..hi];

        // Combine the clip-left-edge skip (when the string starts left of the
        // clip) with the caller's text_indent — both are column skips into `s`.
        let (indent, clip_skip) = if ax >= self.clip.a.x {
            ((ax - self.clip.a.x) as usize, 0)
        } else {
            (0, self.clip.a.x - ax)
        };
        crate::text::draw_str(row, indent, s, text_indent + clip_skip, style) as i32
    }

    /// Write `s` at view-local `(x, y)`, toggling between `lo` and `hi` styles at
    /// each `~` (the `~` itself is not drawn) — the attribute-pair toggle used by
    /// frame icons and reused by buttons/labels/menus for hotkey highlighting.
    /// Starts in `lo`. Clipped exactly like [`put_char`](Self::put_char). Returns
    /// the number of columns advanced.
    ///
    /// Faithful to [`DrawBuffer::move_cstr_part`](crate::screen::DrawBuffer): the
    /// first `~` flips `lo` → `hi`, the next flips back, and so on; the `~`
    /// characters draw nothing and do not advance the column.
    pub fn put_cstr(&mut self, x: i32, y: i32, s: &str, lo: Style, hi: Style) -> i32 {
        let mut col = 0i32;
        let mut current = lo;
        let mut hi_active = false;
        for ch in s.chars() {
            if ch == '~' {
                hi_active = !hi_active;
                current = if hi_active { hi } else { lo };
                continue;
            }
            self.put_char(x + col, y, ch, current);
            col += UnicodeWidthChar::width(ch).unwrap_or(1) as i32;
        }
        col
    }

    /// Fill view-local rect `area_local` (clipped) with `ch` styled `style`.
    pub fn fill(&mut self, area_local: Rect, ch: char, style: Style) {
        if self.clip.is_empty() {
            return;
        }
        // Translate to absolute and clip.
        let mut abs = area_local;
        abs.r#move(self.origin.x, self.origin.y);
        abs.intersect(&self.clip);
        if abs.is_empty() {
            return;
        }
        for ay in abs.a.y..abs.b.y {
            let row = self.buffer.row_mut(ay as u16);
            for ax in abs.a.x..abs.b.x {
                let cell = &mut row[ax as usize];
                cell.set_char(ch);
                cell.set_style(style);
            }
        }
    }

    /// Cast a drop shadow for the view at view-local rect `area_local`.
    ///
    /// The shadow region is `(area_local translated by SHADOW_SIZE) minus
    /// area_local` — the classic offset-L: a 2-column strip down the right
    /// edge (starting 1 row below the top, extending 1 row past the bottom) plus
    /// a 1-row strip along the bottom (starting 2 columns right of the left
    /// edge). Each cell in the region (clipped to `self.clip`) keeps its glyph
    /// and gets the shadow attribute: the theme's [`Role::Shadow`] style, or its
    /// [`reversed`](Style::reversed) form when the cell's background is black (so
    /// the shadow stays visible on black). Cells already marked `no_shadow` are
    /// left untouched (no double-shadow where two shadows overlap); transformed
    /// cells get `no_shadow = true`. The cell's own style modifiers survive — the
    /// recolor re-applies the original style word onto the shadow attribute.
    ///
    /// Under whole-tree redraw the buffer is reset each frame, so `no_shadow`
    /// markers never go stale; later (higher) siblings simply paint over the
    /// shadow cells they occlude.
    ///
    /// # Turbo Vision heritage
    /// Ports the shadow pass `applyShadow` (`tvwrite.cpp`); the on-black reverse is
    /// `reverseAttribute(shadowAttr)` and the modifier-preserving recolor is
    /// `setStyle(attr, style | slNoShadow)`.
    pub fn cast_shadow(&mut self, area_local: Rect) {
        if self.clip.is_empty() {
            return;
        }
        let mut abs = area_local;
        abs.r#move(self.origin.x, self.origin.y);
        let shadow = self.theme.style(Role::Shadow);
        // Right strip: 2 columns right of the view, shifted 1 row down (covers
        // the shadow corner past the bottom edge); bottom strip: 1 row below,
        // starting 2 columns in. Disjoint by construction.
        let right = Rect::new(
            abs.b.x,
            abs.a.y + SHADOW_SIZE.y,
            abs.b.x + SHADOW_SIZE.x,
            abs.b.y + SHADOW_SIZE.y,
        );
        let bottom = Rect::new(
            abs.a.x + SHADOW_SIZE.x,
            abs.b.y,
            abs.b.x,
            abs.b.y + SHADOW_SIZE.y,
        );
        for strip in [right, bottom] {
            let mut s = strip;
            s.intersect(&self.clip);
            if s.is_empty() {
                continue;
            }
            for ay in s.a.y..s.b.y {
                for ax in s.a.x..s.b.x {
                    // Per-cell recolor: a strip boundary may split a wide-char
                    // pair, recoloring only one of its two cells.
                    let cell = self.buffer.get_mut(ax as u16, ay as u16);
                    let old = cell.style();
                    if old.modifiers.no_shadow {
                        continue;
                    }
                    let mut out = if bg_is_black(old.bg) {
                        shadow.reversed()
                    } else {
                        shadow
                    };
                    out.modifiers = old.modifiers;
                    out.modifiers.no_shadow = true;
                    cell.set_style(out); // glyph untouched
                }
            }
        }
    }

    /// A child context for a sub-view at view-local rect `area_local`.
    ///
    /// The child's clip is `self.clip ∩ (area_local translated by origin)`, and
    /// its origin is `self.origin + area_local.a`. The buffer is reborrowed for
    /// the child's shorter lifetime. No re-intersection with the buffer bounds
    /// is needed — `self.clip` is already inside them.
    pub fn sub(&mut self, area_local: Rect) -> DrawCtx<'_> {
        let mut abs = area_local;
        abs.r#move(self.origin.x, self.origin.y);
        let mut clip = self.clip;
        clip.intersect(&abs);
        DrawCtx {
            buffer: &mut *self.buffer,
            clip,
            origin: self.origin + area_local.a,
            theme: self.theme,
        }
    }
}

// ---------------------------------------------------------------------------
// Context — the downward event/update context
// ---------------------------------------------------------------------------

/// The event/update context `handle_event` and capture handlers reach for.
///
/// Built over loop-owned state as **distinct `&mut` fields** (not hidden behind
/// a single getter) so the event loop can borrow them disjointly. The live event
/// loop owns the backing `VecDeque` / [`TimerQueue`] / pending-capture `Vec` and
/// constructs a fresh `Context` per dispatch.
///
/// A synchronous return-valued query to a view by id, or a direct message to one,
/// are **tree-owner** primitives (Group/Program over `find_mut`), *not* `Context`
/// methods — a `Context` deliberately holds no tree to route through. rstv has no
/// consumer for a synchronous return-valued query (the close-the-form veto is
/// realized differently), so these are intentionally absent here.
///
/// # Turbo Vision heritage
/// The downward-passed successor to the data a view reads through its owner
/// pointer — the owner's size and dispatch phase, event put-back, command
/// enable/disable, and broadcasts — gathered into one context handed down the tree
/// instead of reached for upward (deviation D3). Broadcasts carry a `ViewId`
/// subject rather than a raw pointer (deviation D4).
pub struct Context<'a> {
    /// Posted commands / broadcasts, drained by the loop after dispatch.
    out_events: &'a mut VecDeque<Event>,
    /// The loop's timer queue.
    timers: &'a mut TimerQueue,
    /// The clock value sampled for this dispatch pass.
    now_ms: u64,
    /// Deferred effects on loop-owned state ([`Deferred`]) — capture pushes, command
    /// enable/disable, and tree mutations (bounds / state-flag / close). A
    /// downward-borrowed view / capture handler cannot touch the capture stack, the
    /// command set, or the tree inline (see [`Deferred`]); it requests the
    /// effect here and the loop applies the queue *after* the current dispatch. One
    /// channel — adding a capability adds a variant, not a field.
    deferred: &'a mut Vec<Deferred>,
    /// The size of the view's owner (the group currently routing to it), so a child
    /// can reach its owner's size/extent without an up-pointer. Used by a window's
    /// zoom / size-limit logic and the drag limits.
    ///
    /// **Transient routing state**, NOT a loop-owned channel: each
    /// `Group::handle_event` sets it to its own size before delivering to children
    /// and restores it on exit (so nesting root→desktop→window works). It is valid
    /// **only during group-routed dispatch**; a capture handler runs *before* group
    /// routing and sees the default `(0,0)`. That is fine — the drag handler must
    /// capture its limits at *push time* (inside the window's `handle_event`, where
    /// `owner_size` is correctly set), never read them at drag time.
    owner_size: Point,
    /// The focused-dispatch phase for the view currently being routed to — the
    /// downward realization of reading the owner's dispatch phase (set by the group
    /// during the focused-events walk, read by the plain-letter accelerators in
    /// buttons, clusters, and labels). With no up-pointer, the phase rides the
    /// `Context` like [`owner_size`](Self::owner_size): **transient routing
    /// state**, set/restored by `Group::route_event` around each leg of the
    /// focused-events walk, valid only during group-routed dispatch. Defaults to
    /// [`Phase::Focused`].
    phase: Phase,
    /// An owned **snapshot** of the program's disabled-command set (denylist),
    /// backing [`command_enabled`](Self::command_enabled) — the read-only
    /// command-enabled query for views, which hold no `&Program`. Owned (a
    /// cheap clone — the set typically holds ≤ a dozen entries), NOT a
    /// `&CommandSet`: the pump's deferred-apply `Context` is alive while the
    /// `EnableCommand`/`DisableCommand` arms mutate the live set `&mut`, so a
    /// shared borrow would alias (see [`Deferred::UpdateMenu`]). The pump
    /// refreshes it once per `pump_once` ([`set_disabled_commands`](Self::set_disabled_commands));
    /// contexts built outside the pump (tests, ctor plumbing) default to empty =
    /// everything enabled. Snapshot semantics: an enable/disable deferred in the
    /// SAME dispatch becomes visible on the next pump.
    disabled_commands: CommandSet,
    /// Snapshot of the registered internal-clipboard editor ID — `None` when
    /// no internal clipboard is wired (= use OS clipboard). Refreshed once per
    /// `pump_once` pass via [`set_clipboard_snapshot`](Self::set_clipboard_snapshot).
    /// Mirrors the process-global clipboard-editor reference. Snapshot
    /// semantics: a `RegisterClipboardEditor` deferred in the SAME dispatch
    /// becomes visible on the next pump.
    clipboard_editor_id: Option<ViewId>,
    /// Whether the clipboard editor currently has a non-empty selection (snapshot).
    /// Drives the paste-enabled logic: paste is enabled when there is no internal
    /// clipboard, or the clipboard editor has a selection. `false` when no internal
    /// clipboard is registered (snapshot default = OS clipboard = paste always
    /// enabled via the `clipboard_editor_id.is_none()` branch).
    clipboard_has_selection: bool,
}

impl<'a> Context<'a> {
    /// Build an event/update context over the loop-owned state.
    pub fn new(
        out_events: &'a mut VecDeque<Event>,
        timers: &'a mut TimerQueue,
        now_ms: u64,
        deferred: &'a mut Vec<Deferred>,
    ) -> Self {
        Context {
            out_events,
            timers,
            now_ms,
            deferred,
            owner_size: Point::default(),
            phase: Phase::Focused,
            disabled_commands: CommandSet::new(),
            clipboard_editor_id: None,
            clipboard_has_selection: false,
        }
    }

    /// Refresh the disabled-command **snapshot** backing
    /// [`command_enabled`](Self::command_enabled). Called by the pump once per
    /// `pump_once` pass with a clone of the program's live disabled set; a
    /// default-constructed `Context` (tests, ctor plumbing) keeps the empty set
    /// (= everything enabled, matching the denylist default).
    pub fn set_disabled_commands(&mut self, snapshot: CommandSet) {
        self.disabled_commands = snapshot;
    }

    /// Whether `cmd` is currently enabled (view-side, denylist: enabled iff not in
    /// the disabled set) — answered from the
    /// per-pump **snapshot** (see the field doc): a `ctx.enable_command` /
    /// `ctx.disable_command` requested during this dispatch is deferred and
    /// becomes visible here on the *next* pump. Lets a widget self-gray (e.g. a
    /// button checking its own command) without the aliasing problem a live
    /// `&CommandSet` accessor would have.
    pub fn command_enabled(&self, cmd: Command) -> bool {
        !self.disabled_commands.has(cmd)
    }

    /// The registered internal-clipboard editor ID (from the pump snapshot).
    /// `None` = no internal clipboard wired → use the OS clipboard.
    pub fn clipboard_editor_id(&self) -> Option<ViewId> {
        self.clipboard_editor_id
    }

    /// Whether the clipboard editor has a non-empty selection (from the pump
    /// snapshot). Drives the paste-enabled logic in `update_commands`: paste is
    /// enabled when there is no internal clipboard editor, or the registered one
    /// has a selection.
    pub fn clipboard_has_selection(&self) -> bool {
        self.clipboard_has_selection
    }

    /// Refresh the clipboard snapshot. Called by the pump once per `pump_once`
    /// pass with the live `clipboard_editor_id` + `clipboard_has_selection`;
    /// default-constructed contexts (tests, ctor plumbing) keep `None`/`false`
    /// (= OS clipboard, paste always enabled).
    pub fn set_clipboard_snapshot(&mut self, editor_id: Option<ViewId>, has_selection: bool) {
        self.clipboard_editor_id = editor_id;
        self.clipboard_has_selection = has_selection;
    }

    /// Register `editor_id` as the process-wide internal clipboard editor —
    /// **deferred** ([`Deferred::RegisterClipboardEditor`]). The pump stores the
    /// ID, marks the editor `is_clipboard = true`, and sets the `EditWindow`'s
    /// title to "Clipboard".
    pub fn register_clipboard_editor(&mut self, editor_id: ViewId, window_id: ViewId) {
        self.deferred.push(Deferred::RegisterClipboardEditor {
            editor_id,
            window_id,
        });
    }

    /// Copy `data` into the clipboard editor — **deferred**
    /// ([`Deferred::ClipboardEditorReceive`]). The pump finds the clipboard editor
    /// and calls `insert_from(&data)` (which selects the inserted text, since this
    /// is the clipboard editor).
    pub fn clipboard_editor_receive(&mut self, clipboard_id: ViewId, data: Vec<u8>) {
        self.deferred
            .push(Deferred::ClipboardEditorReceive { clipboard_id, data });
    }

    /// Paste from the clipboard editor into `dest_id` — **deferred**
    /// ([`Deferred::ClipboardEditorPaste`]). The pump reads the clipboard editor's
    /// selection bytes, then calls `insert_from(&data)` on the dest editor.
    pub fn clipboard_editor_paste(&mut self, dest_id: ViewId, clipboard_id: ViewId) {
        self.deferred.push(Deferred::ClipboardEditorPaste {
            dest_id,
            clipboard_id,
        });
    }

    /// Post a targeted command (`Event::Command`) into the loop's queue.
    pub fn post(&mut self, cmd: Command) {
        self.out_events.push_back(Event::Command(cmd));
    }

    /// Broadcast a command (`Event::Broadcast`) into the loop's queue. `source`
    /// names the view the broadcast is about (the resolvable subject), or `None`
    /// if it concerns no particular view.
    pub fn broadcast(&mut self, command: Command, source: Option<ViewId>) {
        self.out_events
            .push_back(Event::Broadcast { command, source });
    }

    /// Arm a timer, returning its handle. `now_ms` is supplied from this
    /// context's dispatch snapshot (the clock is not stored in the queue).
    pub fn set_timer(&mut self, timeout: Duration, period: Option<Duration>) -> TimerId {
        self.timers.set_timer(self.now_ms, timeout, period)
    }

    /// Cancel a pending timer.
    pub fn kill_timer(&mut self, id: TimerId) {
        self.timers.kill_timer(id);
    }

    /// Push a capture handler — **deferred** ([`Deferred::PushCapture`]). The loop
    /// applies the queue after the current dispatch, so the pushed handler sees the
    /// *next* event, never the current one.
    ///
    /// There is intentionally **no `pop_capture`**: a handler pops itself by
    /// returning [`CaptureFlow::ConsumedPop`](crate::capture::CaptureFlow::ConsumedPop).
    pub fn push_capture(&mut self, handler: Box<dyn CaptureHandler>) {
        self.deferred.push(Deferred::PushCapture(handler));
    }

    /// Request `cmd` be enabled in the program's command set — **deferred**
    /// ([`Deferred::EnableCommand`]). Lets a view enable a command without an
    /// up-pointer to the program.
    pub fn enable_command(&mut self, cmd: Command) {
        self.deferred.push(Deferred::EnableCommand(cmd));
    }

    /// Request `cmd` be disabled — **deferred** ([`Deferred::DisableCommand`]; see
    /// [`enable_command`](Self::enable_command)).
    pub fn disable_command(&mut self, cmd: Command) {
        self.deferred.push(Deferred::DisableCommand(cmd));
    }

    /// Request the view named by `id` be moved/resized to `bounds` — **deferred**
    /// ([`Deferred::ChangeBounds`]). The loop resolves `id` via `find_mut` and calls
    /// `change_bounds`. A capture handler (the drag) holds only a [`ViewId`],
    /// so it cannot mutate the tree inline.
    pub fn request_bounds(&mut self, id: ViewId, bounds: Rect) {
        self.deferred.push(Deferred::ChangeBounds(id, bounds));
    }

    /// Request a propagating state flag be flipped on the view named by `id` —
    /// **deferred** ([`Deferred::SetState`]; see [`request_bounds`](Self::request_bounds)).
    /// The loop resolves `id` via `find_mut` and calls `set_state` (e.g. clearing
    /// the dragging flag at drag end).
    pub fn request_set_state(&mut self, id: ViewId, flag: StateFlag, enable: bool) {
        self.deferred.push(Deferred::SetState(id, flag, enable));
    }

    /// Request the view named by `id` be removed from whichever group owns it —
    /// **deferred** ([`Deferred::Close`]; see [`request_bounds`](Self::request_bounds)).
    /// The loop resolves it via `remove_descendant` (the close command).
    pub fn request_close(&mut self, id: ViewId) {
        self.deferred.push(Deferred::Close(id));
    }

    /// Request the view named by `id` be focused (selected) within its owning
    /// group — **deferred** ([`Deferred::FocusById`]; see
    /// [`request_close`](Self::request_close)). The loop resolves it via
    /// [`View::focus_descendant`](crate::view::View::focus_descendant) (a label
    /// focusing its linked control). The selectable gate is applied during that
    /// group walk, so the caller (the label) need not — and cannot, holding only
    /// the id — check it.
    pub fn request_focus(&mut self, id: ViewId) {
        self.deferred.push(Deferred::FocusById(id));
    }

    /// Request the `button` be made (or un-made) the dialog's default —
    /// **deferred** ([`Deferred::MakeButtonDefault`]). The pump resolves `button`,
    /// downcasts to [`Button`](crate::widgets::Button), and calls
    /// [`make_default`](crate::widgets::Button::make_default). A leaf view (the
    /// change-directory dialog's directory list, on a focus change) holds only
    /// `&mut Context` and cannot poke its sibling button inline; it requests the
    /// change here.
    pub fn make_button_default(&mut self, button: ViewId, enable: bool) {
        self.deferred
            .push(Deferred::MakeButtonDefault { button, enable });
    }

    /// Request the (modal) loop end with `cmd` — **deferred** ([`Deferred::EndModal`]).
    /// Ends the modal loop from a view with no up-pointer to the program: the
    /// pump sets `Program::end_state` and the nested `exec_view` loop observes it.
    ///
    /// **View-side, deferred.** This is the path a [`View`](crate::view::View)
    /// takes (it holds only `&mut Context`, never `&mut Program`). The owner /
    /// top-level path is the immediate `Program::end_modal`. Rule of thumb:
    /// view → `ctx.end_modal`; owner / top-level → `Program::end_modal`.
    pub fn end_modal(&mut self, cmd: Command) {
        self.deferred.push(Deferred::EndModal(cmd));
    }

    /// Request the scroller `scroller` re-read its scrollbars' values and update
    /// its `delta`/`cursor` — **deferred** ([`Deferred::SyncScrollerDelta`]). The
    /// scroller (a leaf) cannot read its window-frame sibling bars itself; the
    /// pump brokers the read. `h`/`v` are the bar [`ViewId`]s (`None` = no bar).
    pub fn request_sync_scroller_delta(
        &mut self,
        scroller: ViewId,
        h: Option<ViewId>,
        v: Option<ViewId>,
    ) {
        self.deferred
            .push(Deferred::SyncScrollerDelta { scroller, h, v });
    }

    /// Request the scrollbar `id` have its parameters set — **deferred**
    /// ([`Deferred::ScrollBarSetParams`]). Each `None` field is preserved from the
    /// bar's live value at apply time. The scroller (a leaf) cannot mutate its
    /// sibling bar inline.
    #[allow(clippy::too_many_arguments)]
    pub fn request_scroll_bar_params(
        &mut self,
        id: ViewId,
        value: Option<i32>,
        min: Option<i32>,
        max: Option<i32>,
        page_step: Option<i32>,
        arrow_step: Option<i32>,
    ) {
        self.deferred.push(Deferred::ScrollBarSetParams {
            id,
            value,
            min,
            max,
            page_step,
            arrow_step,
        });
    }

    /// Request the view `id` be shown/hidden — **deferred**
    /// ([`Deferred::SetVisible`]). A scroller showing/hiding a sibling scrollbar
    /// (which the scroller, a leaf, cannot reach inline).
    pub fn request_set_visible(&mut self, id: ViewId, visible: bool) {
        self.deferred.push(Deferred::SetVisible(id, visible));
    }

    /// Broker a [`DividerOp`] to a splitter by id — **deferred**
    /// ([`Deferred::SplitterDivider`]). Used by the window resize capture,
    /// which cannot touch the splitter inline (holds only `&mut Context`).
    pub fn splitter_divider(&mut self, splitter: ViewId, op: DividerOp) {
        self.deferred
            .push(Deferred::SplitterDivider { splitter, op });
    }

    /// Request the list viewer `list` re-read its scrollbars' values and update
    /// its `focused`/`top_item`/`indent` — **deferred**
    /// ([`Deferred::SyncListViewer`]). The list (a leaf) cannot read its
    /// window-frame sibling bars itself; the pump brokers the read and calls back
    /// through [`View::apply_list_scroll`](crate::view::View::apply_list_scroll).
    /// `h`/`v` are the bar [`ViewId`]s (`None` = no bar).
    pub fn request_sync_list_viewer(&mut self, list: ViewId, h: Option<ViewId>, v: Option<ViewId>) {
        self.deferred.push(Deferred::SyncListViewer { list, h, v });
    }

    /// Request an outline viewer's `delta` be refreshed from its sibling
    /// scrollbars' live `value`s — **deferred**
    /// ([`Deferred::SyncOutlineViewerDelta`]). The viewer (a leaf) cannot read
    /// its window-frame sibling bars itself; the pump brokers the read and writes
    /// the resulting `(dx, dy)` into the viewer's `delta` (a downcast to `Outline`,
    /// like [`SyncScrollerDelta`](Deferred::SyncScrollerDelta)). `h`/`v` are the bar
    /// [`ViewId`]s (`None` = no bar → 0). Unlike the list-viewer sync this writes
    /// nothing back (the outline viewer has no editor cursor / focus write-back), so
    /// it terminates like the scroller's read-only sync.
    pub fn request_sync_outline_viewer_delta(
        &mut self,
        viewer: ViewId,
        h: Option<ViewId>,
        v: Option<ViewId>,
    ) {
        self.deferred
            .push(Deferred::SyncOutlineViewerDelta { viewer, h, v });
    }

    /// Queue a [`PageStack`](crate::widgets::PageStack) sync (see
    /// [`Deferred::PageStackSync`]). Called by `PageStack::handle_event` on a
    /// `TAB_BAR_CHANGED` broadcast from its bound bar.
    pub fn request_sync_page_stack(&mut self, page_stack: ViewId, tab_bar: ViewId) {
        self.deferred.push(Deferred::PageStackSync {
            page_stack,
            tab_bar,
        });
    }

    /// Request the focused directory entry of the file list `source` be resolved
    /// and written into `subscriber` (the file input line) — **deferred**
    /// ([`Deferred::ResolveFocusedFile`]). The resolve-by-source broker for the
    /// payload-carrying file-focused broadcast: the consumer (a leaf) holds
    /// only `&mut Context` and cannot read its `FileList` sibling, so the pump
    /// brokers the read + write.
    pub fn request_resolve_focused_file(&mut self, subscriber: ViewId, source: ViewId) {
        self.deferred
            .push(Deferred::ResolveFocusedFile { subscriber, source });
    }

    /// Request the menu view `id` regray its menu tree against the program's live
    /// command set — **deferred** ([`Deferred::UpdateMenu`]). The menu view (a
    /// child) cannot read the command set itself; the pump brokers it and
    /// calls back through
    /// [`View::update_menu_commands`](crate::view::View::update_menu_commands).
    /// A menu view requests this by its own id when the command set changes.
    pub fn request_update_menu(&mut self, id: ViewId) {
        self.deferred.push(Deferred::UpdateMenu(id));
    }

    /// Request a [`MenuBox`](crate::menu::MenuBox) be opened over `bounds`
    /// presenting `menu`, stamped with the pre-minted `id` — **deferred**
    /// ([`Deferred::OpenMenuBox`]). The [`MenuSession`](crate::menu::MenuSession)
    /// mints `id` itself (so it knows the box id with no callback) and the pump
    /// builds + inserts the box (no focus move). The submenu-open arm of the
    /// flattened menu interaction.
    pub fn request_open_menu_box(&mut self, id: ViewId, menu: crate::menu::Menu, bounds: Rect) {
        self.deferred
            .push(Deferred::OpenMenuBox { id, menu, bounds });
    }

    /// Request the menu view `id` set its highlight cache to `current`
    /// — **deferred** ([`Deferred::SetMenuCurrent`]). The pump calls back through
    /// [`View::set_menu_current`](crate::view::View::set_menu_current). The
    /// session owns the authoritative highlight and pushes it to the view for
    /// `draw`.
    pub fn request_set_menu_current(&mut self, id: ViewId, current: Option<usize>) {
        self.deferred.push(Deferred::SetMenuCurrent(id, current));
    }

    /// Request a view-triggered history modal be opened over the link `link` —
    /// **deferred** ([`Deferred::OpenHistory`]). The history drop-down icon (a leaf)
    /// holds only the link's id and cannot call `exec_view` (top-level only), so it
    /// requests the open; the pump reads the link, records history, builds the
    /// history window, and stashes it into `Program::pending_modal` for the outer
    /// driver to `exec_view` at top level. `require_focus` gates the keyboard
    /// trigger on the link being focused.
    pub fn request_open_history(&mut self, link: ViewId, history_id: u8, require_focus: bool) {
        self.deferred.push(Deferred::OpenHistory {
            link,
            history_id,
            require_focus,
        });
    }

    /// Request the link's current text be recorded into history —
    /// **deferred** ([`Deferred::RecordHistory`]). The pump resolves the link, reads
    /// its current text, and `history_add`s it to the channel.
    pub fn request_record_history(&mut self, link: ViewId, history_id: u8) {
        self.deferred
            .push(Deferred::RecordHistory { link, history_id });
    }

    /// Request a modal message box be opened from inside a downward-borrowed
    /// `&mut View` — **deferred** ([`Deferred::OpenMessageBox`]; the
    /// async-modal-from-a-view seam, `docs/design/async-modal-from-view.md`).
    /// Validation (a validator error, the
    /// [`FileEditor`](crate::widgets::FileEditor) modified-save prompt) holds only
    /// `&mut Context` and cannot run a nested modal inline, so it requests one; the
    /// pump builds + drives it.
    ///
    /// `answer_to = Some(id)` routes the chosen button [`Command`] back to that view
    /// via [`View::set_modal_answer`](crate::view::View::set_modal_answer), and
    /// `then_command = Some(cmd)` re-posts a focused command afterwards so the
    /// original action re-validates with the cached answer. Both `None` for an
    /// informational (OK-only) box.
    pub fn request_message_box(
        &mut self,
        text: String,
        kind: crate::dialog::MessageBoxKind,
        buttons: crate::dialog::MessageBoxButtons,
        answer_to: Option<ViewId>,
        then_command: Option<Command>,
    ) {
        self.deferred.push(Deferred::OpenMessageBox {
            text,
            kind,
            buttons,
            answer_to,
            then_command,
        });
    }

    /// Start mouse hold-tracking for `view` — the widget-facing entry into a
    /// modal mouse hold. Wraps [`push_capture`](Self::push_capture) with a
    /// [`MouseTrackCapture`](crate::capture::MouseTrackCapture): from the *next*
    /// pump on (the deferred-push latency — so the widget runs its press handling
    /// once before the first forwarded event), every masked
    /// `MouseMove`/`MouseAuto`/`MouseWheel` event — and the terminating `MouseUp`
    /// — is localized against `origin` and delivered straight back to `view`'s
    /// `handle_event` via [`Deferred::MouseTrack`]; everything else is swallowed
    /// (the hold is modal). The widget's own `MouseMove`/`MouseAuto` arms run
    /// while held; its `MouseUp` arm runs once at release.
    ///
    /// `origin` is the absolute screen position of `view`-local `(0, 0)`, cached
    /// by the widget's last `draw` (the `Button::abs_origin` /
    /// `ColorPicker::body_origin` pattern).
    ///
    /// The widget's `MouseUp` arm **must** be guarded by a `tracking` flag set
    /// at `MouseDown` time: `MouseUp` is not gated by `Group::wants`, so a
    /// stray, untracked up delivered via normal routing would otherwise reach
    /// the release arm. See step 5 of `docs/design/mouse-track.md`.
    ///
    /// # Turbo Vision heritage
    /// Replaces entering the blocking hold loop `do { … } while (mouseEvent(event,
    /// mask))` (`tview.cpp:636-643`) — whose body-once-before-the-first-wait shape
    /// is why the capture is pushed deferred.
    pub fn start_mouse_track(
        &mut self,
        view: ViewId,
        origin: Point,
        mask: crate::capture::TrackMask,
    ) {
        self.push_capture(Box::new(crate::capture::MouseTrackCapture::new(
            view, origin, mask,
        )));
    }

    /// Forward a localized mouse event to the tracked view — **deferred**
    /// ([`Deferred::MouseTrack`]). `pub(crate)`: two posters only —
    /// [`MouseTrackCapture`](crate::capture::MouseTrackCapture) (the router), and
    /// `Editor::handle_event`'s wheel-in-hold arm, which uses the same direct
    /// find-then-deliver path to forward a mouse-wheel event to its sibling
    /// scrollbars. Widgets enter tracking via
    /// [`start_mouse_track`](Self::start_mouse_track).
    pub(crate) fn request_mouse_track(&mut self, view: ViewId, event: Event) {
        self.deferred.push(Deferred::MouseTrack { view, event });
    }

    /// Request the pump to open a [`FileDialog`](crate::dialog::FileDialog) for
    /// `editor_id` to pick a save-as filename — **deferred**
    /// ([`Deferred::OpenSaveAsDialog`]). Called from the file editor's save / save-as
    /// path when the buffer is untitled. The pump builds + stashes the dialog; the
    /// `SaveAsPick` completion sets the picked name on the editor and re-injects the
    /// save command.
    pub fn request_save_as_dialog(&mut self, editor_id: ViewId) {
        self.deferred.push(Deferred::OpenSaveAsDialog { editor_id });
    }

    /// Queue [`Deferred::OpenColorDialogForRole`] for the theme editor's per-role
    /// color picker — **deferred**. Called from
    /// [`ThemeEditorBody::handle_event`](crate::dialog::ThemeEditorBody). The pump
    /// builds a "Select Color" dialog seeded with `current` and stashes it into
    /// `pending_modal` with a `ThemeColorPick` completion; on OK the completion
    /// updates the `ThemeEditorBody`'s working theme for `role`/`fg`.
    pub fn open_color_dialog_for_role(
        &mut self,
        editor_id: ViewId,
        role: crate::theme::Role,
        fg: bool,
        current: crate::color::Color,
    ) {
        self.deferred.push(Deferred::OpenColorDialogForRole {
            editor_id,
            role,
            fg,
            current,
        });
    }

    /// Request the Find dialog be opened for `editor_id` — deferred
    /// ([`Deferred::OpenFindDialog`]).
    pub fn open_find_dialog(&mut self, editor_id: ViewId) {
        self.deferred.push(Deferred::OpenFindDialog { editor_id });
    }

    /// Request the Replace dialog be opened for `editor_id` — deferred
    /// ([`Deferred::OpenReplaceDialog`]).
    pub fn open_replace_dialog(&mut self, editor_id: ViewId) {
        self.deferred
            .push(Deferred::OpenReplaceDialog { editor_id });
    }

    /// Request the editor `editor` re-read its scrollbars' values and update its
    /// `delta` — **deferred** ([`Deferred::SyncEditorDelta`]). The editor (a leaf
    /// view) cannot read its window-frame sibling bars itself; the pump brokers the
    /// read. `h`/`v` are the bar [`ViewId`]s (`None` = no bar).
    pub fn request_sync_editor_delta(
        &mut self,
        editor: ViewId,
        h: Option<ViewId>,
        v: Option<ViewId>,
    ) {
        self.deferred
            .push(Deferred::SyncEditorDelta { editor, h, v });
    }

    /// Request the editor's `indicator` display `location`/`modified` —
    /// **deferred** ([`Deferred::IndicatorSetValue`]). The editor (a leaf) cannot
    /// mutate its sibling indicator inline.
    pub fn set_indicator_value(&mut self, indicator: ViewId, location: Point, modified: bool) {
        self.deferred.push(Deferred::IndicatorSetValue {
            indicator,
            location,
            modified,
        });
    }

    /// Request `text` be copied to the system clipboard — **deferred**
    /// ([`Deferred::SetClipboard`]). The pump writes it to the backend clipboard.
    pub fn set_clipboard(&mut self, text: String) {
        self.deferred.push(Deferred::SetClipboard(text));
    }

    /// Request the editor `id` paste the system-clipboard text — **deferred**
    /// ([`Deferred::EditorPaste`]). The pump reads the clipboard and inserts.
    pub fn editor_paste(&mut self, id: ViewId) {
        self.deferred.push(Deferred::EditorPaste(id));
    }

    /// Request the input line `id` paste the system-clipboard text — **deferred**
    /// ([`Deferred::InputLinePaste`]). The pump reads the clipboard and calls
    /// [`InputLine::paste_text`](crate::widgets::InputLine::paste_text).
    pub fn request_input_line_paste(&mut self, id: ViewId) {
        self.deferred.push(Deferred::InputLinePaste(id));
    }

    /// Re-queue a **raw event** into the loop's event queue — the raw-event
    /// sibling of [`post`](Self::post) (which only ever queues an
    /// `Event::Command`). The menu session re-posts the triggering event so
    /// the next pump re-delivers it (e.g. an outside click that should reach the
    /// view recovering focus, or a mouse event on submenu-open). Lands in
    /// `out_events`, drained before the backend is polled.
    pub fn put_event(&mut self, ev: Event) {
        self.out_events.push_back(ev);
    }

    /// The clock value sampled for this dispatch pass.
    pub fn now_ms(&self) -> u64 {
        self.now_ms
    }

    /// The owner's size for the view currently being routed to — the downward
    /// realization of reading the owner's size/extent. See the
    /// [`owner_size`](Self::owner_size) field docs: it is **transient routing
    /// state** set/restored by each [`Group::handle_event`](crate::view::Group)
    /// around delivery, valid only during group-routed dispatch. Defaults to
    /// `(0, 0)`.
    pub fn owner_size(&self) -> Point {
        self.owner_size
    }

    /// Set the owner size for the routed view — called by
    /// [`Group::handle_event`](crate::view::Group) before delivering to children
    /// (set to the group's own size) and to restore it on exit.
    pub fn set_owner_size(&mut self, size: Point) {
        self.owner_size = size;
    }

    /// The focused-dispatch phase for the view currently being routed to —
    /// the downward realization of reading the owner's dispatch phase (no
    /// up-pointer, so the phase rides the `Context`; see the
    /// [`phase`](Self::phase) field docs). Defaults to [`Phase::Focused`] outside a
    /// focused-events walk.
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// Set the dispatch phase for the routed view — called by
    /// `Group::route_event` before each leg of the focused-events walk
    /// (pre-process / focused / post-process) and to restore the saved value on
    /// exit. Leaf views never call this — routing infrastructure only, hence
    /// `pub(crate)`.
    pub(crate) fn set_phase(&mut self, phase: Phase) {
        self.phase = phase;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;

    fn style(fg: u8, bg: u8) -> Style {
        Style::new(Color::Bios(fg), Color::Bios(bg))
    }

    // -- DrawCtx ------------------------------------------------------------

    #[test]
    fn put_char_writes_at_origin_offset() {
        let mut buf = Buffer::new(10, 5);
        let theme = Theme::classic_blue();
        let s = style(0xF, 0x1);
        {
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 10, 5), Point::new(2, 1));
            // local (0,0) -> absolute (2,1)
            ctx.put_char(0, 0, 'X', s);
        }
        assert_eq!(buf.get(2, 1).symbol(), "X");
        assert_eq!(buf.get(2, 1).style(), s);
        // origin cell (0,0) untouched
        assert_eq!(buf.get(0, 0).symbol(), " ");
    }

    #[test]
    fn put_char_outside_clip_is_dropped() {
        let mut buf = Buffer::new(10, 5);
        let theme = Theme::classic_blue();
        {
            // clip only covers columns 2..5, rows 1..3
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(2, 1, 5, 3), Point::new(0, 0));
            ctx.put_char(0, 0, 'A', style(0xF, 0x1)); // outside clip
            ctx.put_char(3, 2, 'B', style(0xF, 0x1)); // inside clip
        }
        assert_eq!(
            buf.get(0, 0).symbol(),
            " ",
            "outside clip must not be written"
        );
        assert_eq!(buf.get(3, 2).symbol(), "B");
    }

    #[test]
    fn put_char_never_writes_out_of_buffer_with_huge_clip() {
        let mut buf = Buffer::new(4, 2);
        let theme = Theme::classic_blue();
        {
            // clip far larger than the buffer; construction intersects it down.
            let mut ctx = DrawCtx::new(
                &mut buf,
                &theme,
                Rect::new(0, 0, 1000, 1000),
                Point::new(0, 0),
            );
            // off the buffer edge -> dropped, no panic
            ctx.put_char(100, 100, 'Z', style(0xF, 0x1));
            ctx.put_char(3, 1, 'Q', style(0xF, 0x1));
        }
        assert_eq!(buf.get(3, 1).symbol(), "Q");
    }

    #[test]
    fn put_char_wide_at_clip_right_edge_degrades_to_space() {
        let mut buf = Buffer::new(10, 1);
        let theme = Theme::classic_blue();
        {
            // clip columns 0..3; place a wide glyph whose lead is at col 2,
            // so its trail (col 3) is outside the clip.
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 3, 1), Point::new(0, 0));
            ctx.put_char(2, 0, '中', style(0xF, 0x1));
        }
        assert_eq!(
            buf.get(2, 0).symbol(),
            " ",
            "wide lead with no room degrades to space"
        );
        assert!(!buf.get(2, 0).is_wide());
        assert_eq!(buf.get(3, 0).symbol(), " ", "outside clip untouched");
    }

    #[test]
    fn put_char_wide_with_room_sets_lead_and_trail() {
        let mut buf = Buffer::new(10, 1);
        let theme = Theme::classic_blue();
        {
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 10, 1), Point::new(0, 0));
            ctx.put_char(1, 0, '中', style(0xF, 0x1));
        }
        assert!(buf.get(1, 0).is_wide());
        assert_eq!(buf.get(1, 0).symbol(), "中");
        assert!(buf.get(2, 0).is_wide_trail());
    }

    #[test]
    fn put_str_writes_and_returns_columns() {
        let mut buf = Buffer::new(10, 2);
        let theme = Theme::classic_blue();
        let s = style(0xF, 0x1);
        let n = {
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 10, 2), Point::new(1, 0));
            ctx.put_str(0, 0, "hi", s)
        };
        assert_eq!(n, 2);
        assert_eq!(buf.get(1, 0).symbol(), "h");
        assert_eq!(buf.get(2, 0).symbol(), "i");
        assert_eq!(buf.get(1, 0).style(), s);
    }

    #[test]
    fn put_str_truncates_at_clip_right_edge() {
        let mut buf = Buffer::new(10, 1);
        let theme = Theme::classic_blue();
        let n = {
            // clip columns 0..4
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 4, 1), Point::new(0, 0));
            ctx.put_str(0, 0, "abcdefgh", style(0xF, 0x1))
        };
        assert_eq!(n, 4, "only the clip width is written");
        assert_eq!(buf.get(3, 0).symbol(), "d");
        // beyond the clip stays blank
        assert_eq!(buf.get(4, 0).symbol(), " ");
    }

    #[test]
    fn put_str_starting_left_of_clip_skips_offscreen_columns() {
        let mut buf = Buffer::new(10, 1);
        let theme = Theme::classic_blue();
        {
            // clip columns 2..10. Draw "abcdef" starting at absolute col 0:
            // columns 0,1 ('a','b') are off the clip left edge and skipped.
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(2, 0, 10, 1), Point::new(0, 0));
            ctx.put_str(0, 0, "abcdef", style(0xF, 0x1));
        }
        assert_eq!(buf.get(0, 0).symbol(), " ");
        assert_eq!(buf.get(1, 0).symbol(), " ");
        assert_eq!(buf.get(2, 0).symbol(), "c");
        assert_eq!(buf.get(3, 0).symbol(), "d");
    }

    #[test]
    fn put_cstr_toggles_style_on_tilde() {
        let mut buf = Buffer::new(10, 1);
        let theme = Theme::classic_blue();
        let lo = style(0xF, 0x1);
        let hi = style(0xA, 0x1);
        let n = {
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 10, 1), Point::new(0, 0));
            // "[~X~]" -> '[' and ']' in lo, 'X' in hi; tildes draw nothing.
            ctx.put_cstr(0, 0, "[~X~]", lo, hi)
        };
        assert_eq!(n, 3, "three visible columns advanced (the ~ draw nothing)");
        assert_eq!(buf.get(0, 0).symbol(), "[");
        assert_eq!(buf.get(0, 0).style(), lo);
        assert_eq!(buf.get(1, 0).symbol(), "X");
        assert_eq!(buf.get(1, 0).style(), hi, "between the ~ the style is hi");
        assert_eq!(buf.get(2, 0).symbol(), "]");
        assert_eq!(
            buf.get(2, 0).style(),
            lo,
            "after the closing ~ the style is lo"
        );
    }

    #[test]
    fn put_cstr_clips_like_put_char() {
        let mut buf = Buffer::new(10, 1);
        let theme = Theme::classic_blue();
        let lo = style(0xF, 0x1);
        let hi = style(0xA, 0x1);
        {
            // clip columns 0..2; "[~X~]" draws '[' at 0, 'X' at 1, ']' at 2 (clipped).
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 2, 1), Point::new(0, 0));
            ctx.put_cstr(0, 0, "[~X~]", lo, hi);
        }
        assert_eq!(buf.get(0, 0).symbol(), "[");
        assert_eq!(buf.get(1, 0).symbol(), "X");
        assert_eq!(buf.get(2, 0).symbol(), " ", "beyond the clip stays blank");
    }

    #[test]
    fn fill_clips_to_clip_rect() {
        let mut buf = Buffer::new(6, 4);
        let theme = Theme::classic_blue();
        let s = style(0x0, 0x3);
        {
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(1, 1, 4, 3), Point::new(0, 0));
            // fill a local rect bigger than the clip
            ctx.fill(Rect::new(0, 0, 6, 4), '.', s);
        }
        // inside the clip
        assert_eq!(buf.get(1, 1).symbol(), ".");
        assert_eq!(buf.get(3, 2).symbol(), ".");
        // outside the clip untouched
        assert_eq!(buf.get(0, 0).symbol(), " ");
        assert_eq!(buf.get(4, 2).symbol(), " ");
        assert_eq!(buf.get(1, 1).style(), s);
    }

    #[test]
    fn sub_narrows_clip_and_shifts_origin() {
        let mut buf = Buffer::new(10, 10);
        let theme = Theme::classic_blue();
        {
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 10, 10), Point::new(0, 0));
            let mut child = ctx.sub(Rect::new(3, 2, 6, 5));
            assert_eq!(child.origin(), Point::new(3, 2));
            assert_eq!(child.clip(), Rect::new(3, 2, 6, 5));
            // child-local (0,0) -> absolute (3,2)
            child.put_char(0, 0, 'C', style(0xF, 0x1));
            // child-local write outside the child clip is dropped
            child.put_char(100, 100, 'X', style(0xF, 0x1));
        }
        assert_eq!(buf.get(3, 2).symbol(), "C");
    }

    #[test]
    fn sub_clip_intersects_parent() {
        let mut buf = Buffer::new(10, 10);
        let theme = Theme::classic_blue();
        {
            // parent clip 2..6 x 2..6
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(2, 2, 6, 6), Point::new(0, 0));
            // child local rect spans 0..10 -> intersect with parent clip
            let child = ctx.sub(Rect::new(0, 0, 10, 10));
            assert_eq!(child.clip(), Rect::new(2, 2, 6, 6));
        }
    }

    #[test]
    fn empty_clip_writes_nothing() {
        let mut buf = Buffer::new(5, 5);
        let theme = Theme::classic_blue();
        {
            // a clip that does not overlap the buffer at all
            let mut ctx = DrawCtx::new(
                &mut buf,
                &theme,
                Rect::new(100, 100, 200, 200),
                Point::new(0, 0),
            );
            assert!(ctx.clip().is_empty());
            ctx.put_char(0, 0, 'X', style(0xF, 0x1));
            ctx.put_str(0, 0, "hello", style(0xF, 0x1));
            ctx.fill(Rect::new(0, 0, 5, 5), '#', style(0xF, 0x1));
        }
        for cell in buf.cells() {
            assert_eq!(cell.symbol(), " ");
        }
    }

    // -- cast_shadow (shadow pass) -----------------------------------------

    /// The classic_blue Shadow style with `no_shadow` set — what a transformed
    /// non-black cell ends up with.
    fn shadow_style(theme: &Theme) -> Style {
        let mut s = theme.style(Role::Shadow);
        s.modifiers.no_shadow = true;
        s
    }

    #[test]
    fn cast_shadow_preserves_glyph_and_applies_shadow_style() {
        let mut buf = Buffer::new(10, 6);
        let theme = Theme::classic_blue();
        // Fill the whole buffer with a non-black-bg pattern.
        let base = style(0x7, 0x1); // lightgray on blue (non-black bg)
        {
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 10, 6), Point::new(0, 0));
            ctx.fill(Rect::new(0, 0, 10, 6), '#', base);
            // View at (1,1)-(5,4); shadow = right strip x 5..7 y 2..5,
            // bottom strip x 3..5 y 4..5.
            ctx.cast_shadow(Rect::new(1, 1, 5, 4));
        }
        let cell = buf.get(5, 2); // inside the right strip
        assert_eq!(cell.symbol(), "#", "glyph must be preserved");
        assert_eq!(cell.style(), shadow_style(&theme));
        let cell = buf.get(4, 4); // inside the bottom strip
        assert_eq!(cell.symbol(), "#");
        assert_eq!(cell.style(), shadow_style(&theme));
    }

    #[test]
    fn cast_shadow_reverses_on_black_background() {
        let mut buf = Buffer::new(10, 6);
        let theme = Theme::classic_blue();
        let black_bg = style(0x7, 0x0); // bg BIOS 0 -> black
        {
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 10, 6), Point::new(0, 0));
            ctx.fill(Rect::new(0, 0, 10, 6), '#', black_bg);
            ctx.cast_shadow(Rect::new(1, 1, 5, 4));
        }
        let mut expected = theme.style(Role::Shadow).reversed();
        expected.modifiers.no_shadow = true;
        assert_eq!(buf.get(5, 2).style(), expected);
        // The default-color background also counts as black (toBIOS(false) -> 0).
        let mut buf = Buffer::new(10, 6);
        {
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 10, 6), Point::new(0, 0));
            ctx.cast_shadow(Rect::new(1, 1, 5, 4)); // over default cells
        }
        assert_eq!(buf.get(5, 2).style(), expected);
    }

    #[test]
    fn cast_shadow_skips_cells_already_marked_no_shadow() {
        let mut buf = Buffer::new(10, 6);
        let theme = Theme::classic_blue();
        let mut marked = style(0x7, 0x1);
        marked.modifiers.no_shadow = true;
        {
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 10, 6), Point::new(0, 0));
            ctx.fill(Rect::new(0, 0, 10, 6), '#', marked);
            ctx.cast_shadow(Rect::new(1, 1, 5, 4));
        }
        // Every shadow-region cell was pre-marked, so nothing changes.
        assert_eq!(buf.get(5, 2).style(), marked, "no double-shadowing");
        assert_eq!(buf.get(4, 4).style(), marked);
    }

    #[test]
    fn cast_shadow_preserves_cell_modifiers() {
        // C++ setStyle(attr, style | slNoShadow): the cell's own style bits
        // survive the colour transform.
        let mut buf = Buffer::new(10, 6);
        let theme = Theme::classic_blue();
        let mut bold = style(0x7, 0x1);
        bold.modifiers.bold = true;
        {
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 10, 6), Point::new(0, 0));
            ctx.fill(Rect::new(0, 0, 10, 6), '#', bold);
            ctx.cast_shadow(Rect::new(1, 1, 5, 4));
        }
        let got = buf.get(5, 2).style();
        let mut expected = shadow_style(&theme);
        expected.modifiers.bold = true;
        assert_eq!(got, expected);
    }

    #[test]
    fn cast_shadow_is_clipped() {
        let mut buf = Buffer::new(10, 6);
        let theme = Theme::classic_blue();
        let base = style(0x7, 0x1);
        {
            // Clip covers only the view itself — both strips fall outside.
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(1, 1, 5, 4), Point::new(0, 0));
            ctx.fill(Rect::new(0, 0, 10, 6), '#', base);
            ctx.cast_shadow(Rect::new(1, 1, 5, 4));
        }
        for y in 0..6u16 {
            for x in 0..10u16 {
                assert!(
                    !buf.get(x, y).style().modifiers.no_shadow,
                    "({x},{y}) must not be shadowed outside the clip"
                );
            }
        }
    }

    #[test]
    fn cast_shadow_geometry_is_the_offset_l() {
        // View at local (1,1)-(5,4), origin (0,0): the shadow is exactly
        //   right strip:  x in [5,7), y in [2,5)
        //   bottom strip: x in [3,5), y in [4,5)
        let mut buf = Buffer::new(10, 6);
        let theme = Theme::classic_blue();
        {
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 10, 6), Point::new(0, 0));
            ctx.fill(Rect::new(0, 0, 10, 6), '#', style(0x7, 0x1));
            ctx.cast_shadow(Rect::new(1, 1, 5, 4));
        }
        let in_l = |x: i32, y: i32| {
            ((5..7).contains(&x) && (2..5).contains(&y))
                || ((3..5).contains(&x) && (4..5).contains(&y))
        };
        for y in 0..6 {
            for x in 0..10 {
                let shadowed = buf.get(x as u16, y as u16).style().modifiers.no_shadow;
                assert_eq!(
                    shadowed,
                    in_l(x, y),
                    "cell ({x},{y}): expected in_shadow={}",
                    in_l(x, y)
                );
            }
        }
    }

    #[test]
    fn cast_shadow_of_zero_size_view_is_noop() {
        let mut buf = Buffer::new(6, 4);
        let theme = Theme::classic_blue();
        let base = style(0x7, 0x1);
        {
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 6, 4), Point::new(0, 0));
            ctx.fill(Rect::new(0, 0, 6, 4), '#', base);
            ctx.cast_shadow(Rect::new(2, 2, 2, 2)); // zero-size view
        }
        for cell in buf.cells() {
            assert!(
                !cell.style().modifiers.no_shadow,
                "no shadow from a zero-size view"
            );
        }
    }

    #[test]
    fn cast_shadow_translates_by_origin() {
        // Same L, but expressed via a ctx origin of (2,1): local (0,0)-(4,3)
        // -> absolute (2,1)-(6,4); right strip x 6..8 y 2..5, bottom x 4..6 y 4..5.
        let mut buf = Buffer::new(10, 6);
        let theme = Theme::classic_blue();
        {
            let mut ctx = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 10, 6), Point::new(2, 1));
            ctx.cast_shadow(Rect::new(0, 0, 4, 3));
        }
        assert!(buf.get(6, 2).style().modifiers.no_shadow);
        assert!(buf.get(7, 4).style().modifiers.no_shadow); // shadow corner
        assert!(buf.get(4, 4).style().modifiers.no_shadow);
        assert!(!buf.get(6, 1).style().modifiers.no_shadow); // above the strip
        assert!(!buf.get(3, 4).style().modifiers.no_shadow); // left of bottom strip
    }

    // -- Context ------------------------------------------------------------

    #[test]
    fn context_post_and_broadcast_land_in_out_events() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            ctx.post(Command::OK);
            ctx.broadcast(Command::QUIT, None);
        }
        assert_eq!(out.len(), 2);
        assert_eq!(out[0], Event::Command(Command::OK));
        assert_eq!(
            out[1],
            Event::Broadcast {
                command: Command::QUIT,
                source: None
            }
        );
    }

    #[test]
    fn context_set_and_kill_timer() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let id = {
            let mut ctx = Context::new(&mut out, &mut timers, 100, &mut deferred);
            assert_eq!(ctx.now_ms(), 100);
            ctx.set_timer(Duration::from_millis(50), None)
        };
        assert_eq!(timers.len(), 1);
        {
            let mut ctx = Context::new(&mut out, &mut timers, 100, &mut deferred);
            ctx.kill_timer(id);
        }
        assert_eq!(timers.len(), 0);
    }

    #[test]
    fn context_command_changes_queue_enable_and_disable() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            ctx.enable_command(Command::OK);
            ctx.disable_command(Command::CANCEL);
        }
        assert_eq!(deferred.len(), 2);
        assert!(matches!(deferred[0], Deferred::EnableCommand(Command::OK)));
        assert!(matches!(
            deferred[1],
            Deferred::DisableCommand(Command::CANCEL)
        ));
    }

    #[test]
    fn context_end_modal_queues_deferred() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            ctx.end_modal(Command::CANCEL);
        }
        assert_eq!(deferred.len(), 1);
        assert!(matches!(deferred[0], Deferred::EndModal(Command::CANCEL)));
    }

    #[test]
    fn context_owner_size_defaults_zero_and_round_trips() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
        // Context::new defaults owner_size to (0, 0).
        assert_eq!(ctx.owner_size(), Point::new(0, 0));
        // The setter round-trips.
        ctx.set_owner_size(Point::new(80, 25));
        assert_eq!(ctx.owner_size(), Point::new(80, 25));
    }

    #[test]
    fn context_phase_defaults_focused_and_round_trips() {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
        // Context::new defaults the dispatch phase to Focused.
        assert_eq!(ctx.phase(), Phase::Focused);
        // The setter round-trips.
        ctx.set_phase(Phase::PostProcess);
        assert_eq!(ctx.phase(), Phase::PostProcess);
        ctx.set_phase(Phase::PreProcess);
        assert_eq!(ctx.phase(), Phase::PreProcess);
    }
}
