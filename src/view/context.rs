//! Downward draw and event/update contexts — deviations **D3** / **D4**.
//!
//! D3 forbids up-pointers: a parent passes a context *down* carrying everything
//! a child would otherwise reach upward for. There are two:
//!
//! * [`DrawCtx`] — the clipped, themed writer a view paints through during
//!   `draw()`. It works in *view-local* coordinates; the ctx translates them to
//!   absolute screen coordinates and clips. It re-expresses the `DrawBuffer`
//!   write ops (D8 clip-for-correctness) on top of the row-18 [`Buffer`] and the
//!   row-8 [`text`](crate::text) primitives — never re-deriving wide-char logic.
//! * [`Context`] — the event/update context handlers and `handle_event` reach
//!   for. It is anchored to the decided `ctx.*` call surface (post / broadcast /
//!   timer scheduling / deferred capture push). It is built over loop-owned
//!   state as **distinct `&mut` fields** so Phase 1 can take disjoint-field
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
// Deferred — an effect on loop-owned state requested through Context (D3 / D9)
// ---------------------------------------------------------------------------

/// An effect on loop-owned state that a downward-borrowed view / capture handler
/// cannot perform inline (D3/D9). During dispatch the view tree is a live `&mut`
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
    /// current one (the `compose_full_protocol` invariant).
    PushCapture(Box<dyn CaptureHandler>),
    /// Enable a command in the program's command set (`enableCommand`).
    EnableCommand(Command),
    /// Disable a command in the program's command set (`disableCommand`).
    DisableCommand(Command),
    /// Apply new bounds to the view named by `ViewId` (drag move/grow). No ctx
    /// needed at apply time (`change_bounds` takes none).
    ChangeBounds(ViewId, Rect),
    /// Flip a propagating state flag on the view (drag end → `sfDragging` off).
    SetState(ViewId, StateFlag, bool),
    /// Remove the view from whichever group owns it (`cmClose`).
    Close(ViewId),
    /// Focus (select) the view named by `ViewId` within its owning group
    /// (`TLabel::focusLink` → `link->focus()`). The pump resolves it via
    /// [`View::focus_descendant`](crate::view::View::focus_descendant), which walks
    /// to the owning group and runs `focus_child` (the `ofSelectable` gate lives in
    /// that group walk, not at the request site). A view (the label) holds only the
    /// link's [`ViewId`] (D3), so it cannot select a sibling inline.
    FocusById(ViewId),
    /// Request the (modal) loop end with `command` (`TGroup::endModal`). The pump
    /// applies it by setting `Program::end_state`; the nested `exec_view` loop then
    /// observes it. The downward (D3) replacement for a view calling `endModal` up
    /// its owner chain.
    ///
    /// This touches **loop state** (`end_state`) — a fourth disjoint target
    /// alongside the capture stack / command set / view tree — so the `69897fe`
    /// insertion-order drain stays order-equivalent: no dispatch co-queues an
    /// `EndModal` with an effect on the *same* state, and cross-family order never
    /// affects the result.
    EndModal(Command),

    // -- row 27: the TScroller cross-view scrollbar broker (D3) --------------
    //
    // All three touch the **view tree** family (same as `ChangeBounds`/`SetState`/
    // `Close`/`FocusById`), so the `69897fe` insertion-order drain stays
    // order-equivalent: no single dispatch co-queues two ops on the *same*
    // scrollbar/scroller in a conflicting order. They exist because a leaf view
    // (the scroller) holds only `&mut Context` (D3) and so can neither **read** nor
    // **mutate** its window-frame sibling scrollbars; the pump — which owns the
    // whole tree — is the cross-view broker, performing every read/write at
    // deferred-apply time via `group.find_mut(id)`.
    /// **Read direction** (`TScroller::scrollDraw`): resolve the `h`/`v` scrollbars,
    /// read each `value` (via [`View::value`](crate::view::View::value) →
    /// [`FieldValue::Int`](crate::data::FieldValue::Int)), and push the resulting
    /// delta into `scroller` (the pump downcasts it to `Scroller` and calls
    /// `apply_delta`, which does the `setCursor` adjust + `delta = d`). The scroller
    /// requests this from `handle_event` when a `cmScrollBarChanged` broadcast names
    /// one of its bars as `source`.
    SyncScrollerDelta {
        /// The scroller whose `delta`/`cursor` to update.
        scroller: ViewId,
        /// The horizontal scrollbar to read `value` from (`None` = no h bar → 0).
        h: Option<ViewId>,
        /// The vertical scrollbar to read `value` from (`None` = no v bar → 0).
        v: Option<ViewId>,
    },
    /// **Write direction** (`TScrollBar::setParams`/`setValue`, driven by
    /// `TScroller::setLimit`/`scrollTo`). The pump resolves `id`, downcasts to
    /// `ScrollBar`, fills each `None` field from the bar's **live** value
    /// (preserve-where-`None`), then calls `set_params` — which clamps and may
    /// re-broadcast `cmScrollBarChanged`. One flexible variant serves row 27 and the
    /// future `TListViewer`/`TEditor` (`setRange`/`setStep`/`setValue` shapes).
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
    /// **Visibility direction** (`TScroller::showSBar` → `TView::show`/`hide`). The
    /// pump resolves `id` and sets `state.state.visible` (no downcast —
    /// `state_mut` is on the trait; the painter skips `!visible` children). There is
    /// no propagating `StateFlag::Visible` (D8 dropped `sfVisible`'s side effects),
    /// so visibility is set directly on the [`ViewState`](crate::view::ViewState).
    SetVisible(ViewId, bool),

    // -- row 28: the TListViewer cross-view scrollbar read-sync (D3) ----------
    /// **Read direction for `TListViewer`** (the `cmScrollBarChanged` handler).
    /// Resolve the `h`/`v` scrollbars, read each `value`
    /// (via [`View::value`](crate::view::View::value) →
    /// [`FieldValue::Int`](crate::data::FieldValue::Int)), then call
    /// [`View::apply_list_scroll`](crate::view::View::apply_list_scroll) on the
    /// `list` view (the trait method — NOT a downcast: `ListViewer` is a trait, so
    /// `dyn View → dyn ListViewer` cannot be downcast, unlike the row-27 scroller).
    ///
    /// **Termination (the centerpiece property):** unlike
    /// [`SyncScrollerDelta`](Self::SyncScrollerDelta), this read-sync **writes
    /// back** — `apply_list_scroll`'s `focus_item_num` calls `focusItem`, which
    /// requests a `setValue(focused)` on the v-bar (another
    /// [`ScrollBarSetParams`](Self::ScrollBarSetParams)). That terminates because
    /// [`ScrollBar::set_params`](crate::widgets::ScrollBar::set_params) is
    /// **change-guarded**: it re-broadcasts `cmScrollBarChanged` only on an actual
    /// value change, so writing back the already-current value is a silent no-op
    /// (steady state: quiescent; after a clamp: one extra round then quiescent).
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

    // -- row 89: the TOutlineViewer scrollbar read-sync (D3) ------------------
    /// **Read-direction sync for `TOutlineViewer`** (ports the `cmScrollBarChanged`
    /// case of `TOutlineViewer::handleEvent`, inherited from `TScroller`). The pump
    /// resolves both bars, reads each `value` (via [`View::value`] →
    /// [`FieldValue::Int`](crate::data::FieldValue::Int)), and writes the resulting
    /// `(dx, dy)` into `viewer`'s `delta` (the pump downcasts it to `Outline` and
    /// calls `apply_delta`). Like [`SyncScrollerDelta`](Self::SyncScrollerDelta) this
    /// is **read-only** — it writes nothing back to the bars, so it terminates with
    /// no change-guard needed (unlike [`SyncListViewer`](Self::SyncListViewer)).
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

    // -- row 49: the TMenuView command-graying broker (D3) --------------------
    /// **Command-graying broker for `TMenuView`** (ports `updateMenu`, triggered
    /// by the `cmCommandSetChanged` broadcast). Resolve the menu view by `id` and
    /// call [`View::update_menu_commands`](crate::view::View::update_menu_commands)
    /// with the pump's **live** [`CommandSet`](crate::command::CommandSet), which
    /// regrays the menu tree (`disabled = !commandEnabled(command)` per command
    /// item, recursing submenus).
    ///
    /// A broker — **not** a `&CommandSet` read-accessor on [`Context`] — because
    /// the command set lives on `Program` and the apply-phase `Context` is alive
    /// across a loop whose `EnableCommand`/`DisableCommand` arms mutate
    /// `disabled_commands` (`&mut`); a `&CommandSet` on `Context` would alias
    /// that borrow. The view (a child, D3) cannot read the command set inline, so
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

    // -- rows 50-52: the TMenuView modal layer (MenuSession, D3/D9) ------------
    /// **Open a menu box** — the deferred realization of `execute()`'s submenu
    /// open (`tmnuview.cpp:382`, `topMenu()->newSubView(r, current->subMenu)` →
    /// `owner->execView(target)`). The [`MenuSession`](crate::menu::MenuSession)
    /// capture handler **pre-mints** `id` from [`ViewId::next`](crate::view::ViewId)
    /// so it already knows the box id with no insert-time callback; the pump
    /// builds a [`MenuBox`](crate::menu::MenuBox) from `menu` over `bounds` and
    /// [`Group::insert_with_id`](crate::view::Group::insert_with_id)s it into the
    /// root group, stamping that id. **No focus move** — the box is never current
    /// (Clean Architecture A; the session owns every event). `menu` is a clone of
    /// the submenu subtree (clone-at-open is faithful — `execute()` has no
    /// evBroadcast case, so `disabled` is frozen for the box's lifetime).
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
    /// **Set a menu view's highlight cache** (`TMenuView::current` ← index). The
    /// pump resolves `id` and calls
    /// [`View::set_menu_current`](crate::view::View::set_menu_current) (a trait
    /// method, mirroring the `update_menu_commands` broker — no downcast). This is
    /// the write-only display cache the bar/box `draw` reads to pick the selected
    /// colour; the [`MenuSession`](crate::menu::MenuSession) owns the authoritative
    /// `current` and pushes it here whenever navigation moves the highlight.
    ///
    /// Touches the **view-tree** family, so the insertion-order drain stays
    /// order-equivalent.
    SetMenuCurrent(ViewId, Option<usize>),

    // -- row 57: the THistory view-triggered async-modal seam (D3/D9) ----------
    /// **View-triggered modal open** (`THistory`; msgbox 63 will add sibling
    /// completions). Built at apply time because the trigger view holds only the
    /// link's id (D3): the pump reads the link, records history, builds the
    /// `THistoryWindow`, and stashes it into `Program::pending_modal` — it does
    /// **not** call `exec_view` here (the apply phase is inside the `pump_once`
    /// destructure; a view cannot call `exec_view`, which is top-level only). The
    /// OUTER driver loop runs `exec_view` at top level after `pump_once` returns.
    ///
    /// Touches the **view-tree** family + **loop state** (`pending_modal`), like
    /// the other tree ops + `EndModal`, so the insertion-order drain stays
    /// order-equivalent (no dispatch co-queues a conflicting op on the same state).
    OpenHistory {
        /// The linked `TInputLine` whose text/bounds/focus drive the open + flowback.
        link: ViewId,
        /// The history channel id.
        history_id: u8,
        /// True for the keyboard trigger (gate on the link being focused, faithful
        /// to `(link->state & sfFocused)`); false for the mouse trigger.
        require_focus: bool,
    },
    /// **recordHistory(link->data)** for the broadcast arm (`cmReleasedFocus` on
    /// the link / `cmRecordHistory`): resolve the link, read its text,
    /// `history_add(id, text)`. Touches no loop-owned state beyond the read of the
    /// view tree (a pure side effect on the process-global history store), so it is
    /// order-equivalent with every other family.
    RecordHistory { link: ViewId, history_id: u8 },

    // -- row 66: the TEditor cross-view brokers (D3) --------------------------
    /// **Read direction for `TEditor`** (the `cmScrollBarChanged` handler →
    /// `checkScrollBar`). Resolve the `h`/`v` scrollbars, read each `value`
    /// (via [`View::value`](crate::view::View::value)), downcast `editor` to
    /// [`Editor`](crate::widgets::Editor) and call `apply_scroll_delta(dx, dy)` —
    /// the body of C++ `checkScrollBar` (`if value != delta { delta = value;
    /// update(ufView) }`). The editor is **not** a `Scroller`, so it cannot reuse
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
    /// **Indicator write** (`TEditor::doUpdate` → `indicator->setValue`). Resolve
    /// `indicator`, downcast to [`Indicator`](crate::widgets::Indicator), and call
    /// `set_value(location, modified)`. The editor (a leaf, D3) cannot mutate its
    /// sibling indicator inline. Touches the **view-tree** family.
    IndicatorSetValue {
        /// The indicator to update.
        indicator: ViewId,
        /// The cursor position to display (`curPos`).
        location: Point,
        /// Whether the buffer has unsaved changes.
        modified: bool,
    },
    /// **Copy text to the system clipboard** (`TEditor::clipCopy`, the
    /// `clipboard == 0` branch → `TClipboard::setText`). The pump calls
    /// `renderer.backend_mut().set_clipboard(&s)`. Touches the backend only, so it
    /// is order-equivalent with every family.
    SetClipboard(String),
    /// **Paste from the system clipboard** (`TEditor::clipPaste`, the
    /// `clipboard == 0` branch → `TClipboard::requestText`). The pump reads
    /// `renderer.backend_mut().get_clipboard()`, downcasts `editor` to
    /// [`Editor`](crate::widgets::Editor), and inserts the text. Touches the
    /// **view-tree** family + the backend.
    EditorPaste(ViewId),
    /// **Paste from the system clipboard into an `InputLine`** (B3 — the
    /// `TInputLine::handleEvent cmPaste` arm → `TClipboard::requestText`).
    /// The pump reads `renderer.backend_mut().get_clipboard()`, downcasts the
    /// view named by `id` to [`InputLine`](crate::widgets::InputLine) via
    /// `as_any_mut`, and calls
    /// [`paste_text`](crate::widgets::InputLine::paste_text) — which inserts at
    /// the cursor, replacing any selection and clamping to `max_len`. Touches
    /// the **view-tree** family + the backend (same as
    /// [`EditorPaste`](Self::EditorPaste)).
    InputLinePaste(ViewId),

    // -- row 77: the payload-carrying-broadcast (cmFileFocused) broker (D3/D4) -
    /// **Resolve a `cmFileFocused` broadcast's `TSearchRec` payload** (the
    /// `TFileInputLine`/`TFileInfoPane` consumers). rstv's
    /// [`Event::Broadcast`](crate::event::Event::Broadcast) is payload-less (D4:
    /// `source` is the resolvable subject, NOT a value carrier), so this is the
    /// resolve-by-source broker — the same shape as
    /// [`SyncListViewer`](Self::SyncListViewer)'s read+write, but reading a
    /// `SearchRec` rather than a scrollbar value.
    ///
    /// The producer ([`FileList`](crate::dialog::FileList)) broadcasts
    /// `FILE_FOCUSED { source = its own id }`; the consumer (a leaf holding only
    /// `&mut Context`, D3, so it cannot read its `FileList` sibling) filters on the
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
        /// The consumer view to write the focused record into (`TFileInputLine`).
        subscriber: ViewId,
        /// The producer view (`TFileList`) whose focused `SearchRec` to read.
        source: ViewId,
    },

    // -- row 80: the TDirListBox → chDirButton makeDefault broker (D3) ---------
    /// **Make a sibling [`Button`](crate::widgets::Button) the default** on a
    /// dir-list focus change (`TDirListBox::setState` →
    /// `((TChDirDialog*)owner)->chDirButton->makeDefault(enable)`). The dir list
    /// is a leaf holding only `&mut Context` (D3), so it cannot reach its sibling
    /// button inline; it queues this and the pump resolves `button`, downcasts to
    /// [`Button`](crate::widgets::Button), and calls
    /// [`make_default`](crate::widgets::Button::make_default) (which re-broadcasts
    /// `cmGrabDefault`/`cmReleaseDefault` so the real default button relinquishes /
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

    // -- colorpick: the color-picker drag broker (D3) -------------------------
    //
    // The color picker is one view (Approach A), so a leaf surface cannot reach
    // the picker's `apply_drag` inline under D3 — it holds only `&mut Context`.
    // `ColorDragCapture` posts this on each `MouseMove`/`MouseUp`; the pump
    // resolves `picker`, downcasts to `ColorPicker` via `as_any_mut`, and calls
    // `apply_drag(pos)`. The region being scrubbed lives in the picker's own
    // `active_drag` field (set when the capture was pushed) — so neither this
    // variant nor the capture handler carries a widget-layer type. Same family
    // as the scroller/list broker ops (view-tree), so the insertion-order drain
    // stays order-equivalent.
    /// **Color-picker drag broker** (the picker is one view, so a leaf surface
    /// can't reach the picker's `apply_drag` inline — D3). The drag capture
    /// handler posts this on each `MouseMove`/`MouseUp`; the pump resolves
    /// `picker`, downcasts to
    /// [`ColorPicker`](crate::dialog::ColorPicker) via `as_any_mut`, and calls
    /// `apply_drag(pos)` (which reads the picker's own `active_drag` region).
    /// `pos` is **picker-local** (the handler converted from absolute via the
    /// picker's cached `body_origin`). Same family (view tree) as the scroller
    /// brokers.
    ColorPickerDrag {
        /// The picker whose active surface to scrub.
        picker: ViewId,
        /// Picker-local pointer position.
        pos: Point,
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
        /// action (e.g. cmClose) re-runs `valid()` with the cached answer. `None`
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
    /// completion sets the chosen filename on the editor and re-injects `cmSave`.
    OpenSaveAsDialog {
        /// The [`FileEditor`](crate::widgets::FileEditor) to save the picked name to.
        editor_id: ViewId,
    },

    // -- A3: the mouse hold-tracking router (D9 MouseTrackCapture seam) -------
    /// **Deliver a localized mouse event to the tracked view** while a mouse
    /// button is held. Posted only by
    /// [`MouseTrackCapture`](crate::capture::MouseTrackCapture) — the D9
    /// successor of the C++ `do { … } while (mouseEvent(event, mask))` blocking
    /// hold-loop (`tview.cpp:636-643`) — for each masked `MouseMove` /
    /// `MouseAuto` / wheel pseudo-down and for the terminating `MouseUp`. The
    /// pump resolves `view` via `group.find_mut` and calls
    /// `handle_event(&mut event, …)` directly (the apply-time analogue of the
    /// outside-modal redirect): the widget's `MouseMove`/`MouseAuto`/`MouseUp`
    /// arms ARE the C++ loop body / post-loop code, so no widget downcast is
    /// needed here (decisive for trait-object viewers like `ListViewer` /
    /// `Outline`). `event` is already **view-local** (the capture subtracted
    /// the origin cached at push time). Touches the **view-tree** family, so
    /// the insertion-order drain stays order-equivalent.
    ///
    /// Direct delivery deliberately bypasses the `Group::wants` event-mask gate
    /// — faithful: the C++ hold loop reads events straight off the queue, not
    /// through the tracked view's `eventMask`.
    MouseTrack {
        /// The view being mouse-tracked (the one that pushed the capture).
        view: ViewId,
        /// The localized event to deliver (`MouseMove`/`MouseAuto`/wheel
        /// `MouseDown`/`MouseUp`, position already view-local).
        event: Event,
    },
}

// ---------------------------------------------------------------------------
// DrawCtx — the downward draw context (D3 / D8)
// ---------------------------------------------------------------------------

/// `shadowSize` (tview.cpp:35) — the drop-shadow offset: 2 columns right,
/// 1 row down.
pub const SHADOW_SIZE: Point = Point::new(2, 1);

/// True iff `bg` counts as black for the shadow transform — the C++ test
/// `getBack(attr).toBIOS(false) != 0` in `applyShadow` (tvwrite.cpp), where
/// `TColorDesired::toBIOS(false)` (colors.h:416) maps BIOS → `b & 0xF` and
/// **Default → 0 (black)**. The xterm-256/RGB quantization ladder lives in the
/// backend per D6, so this is a documented simplification: only the exact
/// black values (`Indexed(0)`/`Indexed(16)`, `Rgb(0,0,0)`) count as black;
/// near-black values the ladder would quantize to BIOS 0 do not.
fn bg_is_black(bg: Color) -> bool {
    match bg {
        Color::Default => true, // toBIOS(false) maps Default → 0 (see fn-level doc)
        Color::Bios(b) => b & 0xF == 0,
        Color::Indexed(i) => i == 0 || i == 16,
        Color::Rgb(r, g, b) => (r, g, b) == (0, 0, 0),
    }
}

/// The clipped, themed writer every view paints through (D3).
///
/// All public write methods take **view-local** coordinates: `(0, 0)` is the
/// view's own top-left. The ctx adds [`origin`](Self::origin) to translate into
/// absolute screen columns/rows, and clips every write to [`clip`](Self::clip).
/// The clip is stored as an **absolute** rect already intersected with the
/// buffer bounds at construction, so a write can never index the buffer out of
/// range.
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

    /// The theme's glyph holder (D7 stub for now).
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
    /// display column `text_indent` of `s` (skipping that many leading columns)
    /// — ports `TDrawBuffer::moveStr`'s `begin` parameter, used by
    /// `TInputLine::draw` to render a horizontally-scrolled field. Width-aware and
    /// clipped exactly like [`put_str`](Self::put_str). Returns columns written.
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
    /// each `~` (the `~` itself is not drawn) — ports `TDrawBuffer::moveCStr`'s
    /// attribute-pair toggle (used by frame icons; reused by buttons/labels/menus
    /// for hotkey highlighting). Starts in `lo`. Clipped exactly like
    /// [`put_char`](Self::put_char). Returns the number of columns advanced.
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

    /// Cast a drop shadow for the view at view-local rect `area_local` — the D8
    /// realization of the C++ shadow pass (`applyShadow`, tvwrite.cpp).
    ///
    /// The shadow region is `(area_local translated by SHADOW_SIZE) minus
    /// area_local` — the classic TV offset-L: a 2-column strip down the right
    /// edge (starting 1 row below the top, extending 1 row past the bottom) plus
    /// a 1-row strip along the bottom (starting 2 columns right of the left
    /// edge). Each cell in the region (clipped to `self.clip`) keeps its glyph
    /// and gets the shadow attribute: the theme's [`Role::Shadow`] style, or its
    /// [`reversed`](Style::reversed) form when the cell's background is black
    /// (`reverseAttribute(shadowAttr)` — so the shadow stays visible on black).
    /// Cells already marked `no_shadow` are left untouched (no double-shadow
    /// where two shadows overlap); transformed cells get `no_shadow = true`.
    /// The cell's own style modifiers survive (C++ `setStyle(attr, style |
    /// slNoShadow)` re-applies the original style word onto the shadow attr).
    ///
    /// Under D8 whole-tree redraw the buffer is reset each frame, so `no_shadow`
    /// markers never go stale; later (higher) siblings simply paint over the
    /// shadow cells they occlude.
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
                    // Per-cell like C++ TVWrite: a strip boundary may split a
                    // wide-char pair, recoloring only one of its two cells.
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
// Context — the downward event/update context (D3 / D4)
// ---------------------------------------------------------------------------

/// The event/update context `handle_event` and capture handlers reach for (D3).
///
/// Built over loop-owned state as **distinct `&mut` fields** (not hidden behind
/// a single getter) so Phase 1 can borrow them disjointly. The live event loop
/// (row 31) owns the backing `VecDeque` / [`TimerQueue`] / pending-capture
/// `Vec` and constructs a fresh `Context` per dispatch.
///
/// `query(ViewId, …) -> Option<T>` / `message(ViewId, …)` are **tree-owner**
/// primitives (Group/Program over `find_mut`), *not* `Context` methods — a
/// `Context` deliberately holds no tree to route through. They are **deferred to
/// row 34** (their first return-consumer, a dialog `cmCanCloseForm` veto), so
/// they are intentionally not stubbed here.
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
    /// command set, or the tree inline (D3/D9; see [`Deferred`]); it requests the
    /// effect here and the loop applies the queue *after* the current dispatch. One
    /// channel — adding a capability adds a variant, not a field.
    deferred: &'a mut Vec<Deferred>,
    /// The size of the view's owner (the group currently routing to it), so a child
    /// can reach `owner->size` / `owner->getExtent()` without an up-pointer (D3).
    /// Used by `TWindow::zoom`/`sizeLimits` (33c) and the drag limits (33d).
    ///
    /// **Transient routing state**, NOT a loop-owned channel: each
    /// `Group::handle_event` sets it to its own size before delivering to children
    /// and restores it on exit (so nesting root→desktop→window works). It is valid
    /// **only during group-routed dispatch**; a capture handler runs *before* group
    /// routing and sees the default `(0,0)`. That is fine — 33d's drag handler must
    /// capture its limits at *push time* (inside the window's `handle_event`, where
    /// `owner_size` is correctly set), never read them at drag time.
    owner_size: Point,
    /// The focused-dispatch phase for the view currently being routed to —
    /// the downward realization of the C++ `owner->phase` read
    /// (`TGroup::phase`, set in `tgroup.cpp:362-371`; read by the plain-letter
    /// accelerators in `tbutton.cpp:219` / `tcluster.cpp:263` /
    /// `tlabel.cpp:94`). C++ exposes `owner->phase`; rstv has no up-pointer
    /// (D3), so the phase rides the `Context` like [`owner_size`](Self::owner_size):
    /// **transient routing state**, set/restored by `Group::route_event` around
    /// each leg of the focused-events walk, valid only during group-routed
    /// dispatch. Defaults to [`Phase::Focused`] (the `TGroup` ctor init,
    /// `tgroup.cpp:28`).
    phase: Phase,
    /// An owned **snapshot** of the program's disabled-command set (denylist,
    /// D1), backing [`command_enabled`](Self::command_enabled) — the read-only
    /// `TView::commandEnabled` for views, which hold no `&Program`. Owned (a
    /// cheap clone — the set typically holds ≤ a dozen entries), NOT a
    /// `&CommandSet`: the pump's deferred-apply `Context` is alive while the
    /// `EnableCommand`/`DisableCommand` arms mutate the live set `&mut`, so a
    /// shared borrow would alias (see [`Deferred::UpdateMenu`]). The pump
    /// refreshes it once per `pump_once` ([`set_disabled_commands`](Self::set_disabled_commands));
    /// contexts built outside the pump (tests, ctor plumbing) default to empty =
    /// everything enabled. Snapshot semantics: an enable/disable deferred in the
    /// SAME dispatch becomes visible on the next pump.
    disabled_commands: CommandSet,
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

    /// Whether `cmd` is currently enabled (`TView::commandEnabled`, view-side,
    /// D1 denylist: enabled iff not in the disabled set) — answered from the
    /// per-pump **snapshot** (see the field doc): a `ctx.enable_command` /
    /// `ctx.disable_command` requested during this dispatch is deferred and
    /// becomes visible here on the *next* pump. Lets a widget self-gray (e.g. a
    /// button checking its own command) without the aliasing problem a live
    /// `&CommandSet` accessor would have.
    pub fn command_enabled(&self, cmd: Command) -> bool {
        !self.disabled_commands.has(cmd)
    }

    /// Post a targeted command (`Event::Command`) into the loop's queue.
    pub fn post(&mut self, cmd: Command) {
        self.out_events.push_back(Event::Command(cmd));
    }

    /// Broadcast a command (`Event::Broadcast`) into the loop's queue. `source`
    /// names the view the broadcast is about (the `infoPtr` successor; D4
    /// amendment), or `None` if it concerns no particular view.
    pub fn broadcast(&mut self, command: Command, source: Option<ViewId>) {
        self.out_events
            .push_back(Event::Broadcast { command, source });
    }

    /// Arm a timer, returning its handle. `now_ms` is supplied from this
    /// context's dispatch snapshot (D9: clock not stored in the queue).
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
    /// ([`Deferred::EnableCommand`]). Realizes `TView::enableCommand` from a view
    /// that has no up-pointer to the program (D3).
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
    /// `change_bounds`. A capture handler (the drag) holds only a [`ViewId`] (D3),
    /// so it cannot mutate the tree inline.
    pub fn request_bounds(&mut self, id: ViewId, bounds: Rect) {
        self.deferred.push(Deferred::ChangeBounds(id, bounds));
    }

    /// Request a propagating state flag be flipped on the view named by `id` —
    /// **deferred** ([`Deferred::SetState`]; see [`request_bounds`](Self::request_bounds)).
    /// The loop resolves `id` via `find_mut` and calls `set_state` (drag end →
    /// `sfDragging` off).
    pub fn request_set_state(&mut self, id: ViewId, flag: StateFlag, enable: bool) {
        self.deferred.push(Deferred::SetState(id, flag, enable));
    }

    /// Request the view named by `id` be removed from whichever group owns it —
    /// **deferred** ([`Deferred::Close`]; see [`request_bounds`](Self::request_bounds)).
    /// The loop resolves it via `remove_descendant` (`cmClose`).
    pub fn request_close(&mut self, id: ViewId) {
        self.deferred.push(Deferred::Close(id));
    }

    /// Request the view named by `id` be focused (selected) within its owning
    /// group — **deferred** ([`Deferred::FocusById`]; see
    /// [`request_close`](Self::request_close)). The loop resolves it via
    /// [`View::focus_descendant`](crate::view::View::focus_descendant)
    /// (`TLabel::focusLink`). The `ofSelectable` gate is applied during that group
    /// walk, so the caller (the label) need not — and cannot, holding only the id —
    /// check it.
    pub fn request_focus(&mut self, id: ViewId) {
        self.deferred.push(Deferred::FocusById(id));
    }

    /// Request the `button` be made (or un-made) the dialog's default —
    /// **deferred** ([`Deferred::MakeButtonDefault`]). The pump resolves `button`,
    /// downcasts to [`Button`](crate::widgets::Button), and calls
    /// [`make_default`](crate::widgets::Button::make_default). A leaf view (the
    /// `TChDirDialog` dir list, on a focus change) holds only `&mut Context` and
    /// cannot poke its sibling button inline (D3); it requests the change here.
    pub fn make_button_default(&mut self, button: ViewId, enable: bool) {
        self.deferred
            .push(Deferred::MakeButtonDefault { button, enable });
    }

    /// Request the (modal) loop end with `cmd` — **deferred** ([`Deferred::EndModal`]).
    /// `TGroup::endModal` from a view with no up-pointer to the program (D3): the
    /// pump sets `Program::end_state` and the nested `exec_view` loop observes it.
    ///
    /// **View-side, deferred.** This is the path a [`View`](crate::view::View)
    /// takes (it holds only `&mut Context`, never `&mut Program`). The owner /
    /// top-level path is the immediate `Program::end_modal`. Rule of thumb:
    /// view → `ctx.end_modal`; owner / top-level → `Program::end_modal`.
    pub fn end_modal(&mut self, cmd: Command) {
        self.deferred.push(Deferred::EndModal(cmd));
    }

    /// Request the `TScroller` `scroller` re-read its scrollbars' values and update
    /// its `delta`/`cursor` — **deferred** ([`Deferred::SyncScrollerDelta`]). The
    /// scroller (a leaf, D3) cannot read its window-frame sibling bars itself; the
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
    /// bar's live value at apply time (`TScrollBar::setParams`/`setValue` driven by
    /// `TScroller::setLimit`/`scrollTo`). The scroller (a leaf, D3) cannot mutate its
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
    /// ([`Deferred::SetVisible`]). `TScroller::showSBar` → `TView::show`/`hide` on a
    /// sibling scrollbar (which the scroller, a leaf, cannot reach inline, D3).
    pub fn request_set_visible(&mut self, id: ViewId, visible: bool) {
        self.deferred.push(Deferred::SetVisible(id, visible));
    }

    /// Request the `TListViewer` `list` re-read its scrollbars' values and update
    /// its `focused`/`top_item`/`indent` — **deferred**
    /// ([`Deferred::SyncListViewer`]). The list (a leaf, D3) cannot read its
    /// window-frame sibling bars itself; the pump brokers the read and calls back
    /// through [`View::apply_list_scroll`](crate::view::View::apply_list_scroll).
    /// `h`/`v` are the bar [`ViewId`]s (`None` = no bar).
    pub fn request_sync_list_viewer(&mut self, list: ViewId, h: Option<ViewId>, v: Option<ViewId>) {
        self.deferred.push(Deferred::SyncListViewer { list, h, v });
    }

    /// Request a `TOutlineViewer`'s `delta` be refreshed from its sibling
    /// scrollbars' live `value`s — **deferred**
    /// ([`Deferred::SyncOutlineViewerDelta`]). The viewer (a leaf, D3) cannot read
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

    /// Request the focused `SearchRec` of the `TFileList` `source` be resolved and
    /// written into `subscriber` (`TFileInputLine`) — **deferred**
    /// ([`Deferred::ResolveFocusedFile`]). The resolve-by-source broker for the
    /// payload-carrying `cmFileFocused` broadcast: the consumer (a leaf, D3) holds
    /// only `&mut Context` and cannot read its `FileList` sibling, so the pump
    /// brokers the read + write.
    pub fn request_resolve_focused_file(&mut self, subscriber: ViewId, source: ViewId) {
        self.deferred
            .push(Deferred::ResolveFocusedFile { subscriber, source });
    }

    /// Request the menu view `id` regray its menu tree against the program's live
    /// command set — **deferred** ([`Deferred::UpdateMenu`]). The menu view (a
    /// child, D3) cannot read the command set itself; the pump brokers it and
    /// calls back through
    /// [`View::update_menu_commands`](crate::view::View::update_menu_commands).
    /// `TMenuView`'s `cmCommandSetChanged` handler requests this by its own id.
    pub fn request_update_menu(&mut self, id: ViewId) {
        self.deferred.push(Deferred::UpdateMenu(id));
    }

    /// Request a color-picker drag update — **deferred**
    /// ([`Deferred::ColorPickerDrag`]). Posted by the picker's drag capture
    /// handler on each `MouseMove`/`MouseUp`.
    pub fn request_color_drag(&mut self, picker: ViewId, pos: Point) {
        self.deferred
            .push(Deferred::ColorPickerDrag { picker, pos });
    }

    /// Request a [`MenuBox`](crate::menu::MenuBox) be opened over `bounds`
    /// presenting `menu`, stamped with the pre-minted `id` — **deferred**
    /// ([`Deferred::OpenMenuBox`]). The [`MenuSession`](crate::menu::MenuSession)
    /// mints `id` itself (so it knows the box id with no callback) and the pump
    /// builds + inserts the box (no focus move). The submenu-open arm of the
    /// flattened `execute()`.
    pub fn request_open_menu_box(&mut self, id: ViewId, menu: crate::menu::Menu, bounds: Rect) {
        self.deferred
            .push(Deferred::OpenMenuBox { id, menu, bounds });
    }

    /// Request the menu view `id` set its highlight cache (`current`) to `current`
    /// — **deferred** ([`Deferred::SetMenuCurrent`]). The pump calls back through
    /// [`View::set_menu_current`](crate::view::View::set_menu_current). The
    /// session owns the authoritative `current` and pushes it to the view for
    /// `draw`.
    pub fn request_set_menu_current(&mut self, id: ViewId, current: Option<usize>) {
        self.deferred.push(Deferred::SetMenuCurrent(id, current));
    }

    /// Request a view-triggered history modal be opened over the link `link` —
    /// **deferred** ([`Deferred::OpenHistory`]). The `THistory` icon (a leaf, D3)
    /// holds only the link's id and cannot call `exec_view` (top-level only), so it
    /// requests the open; the pump reads the link, records history, builds the
    /// `THistoryWindow`, and stashes it into `Program::pending_modal` for the outer
    /// driver to `exec_view` at top level. `require_focus` gates the keyboard
    /// trigger on the link being focused (faithful to `(link->state & sfFocused)`).
    pub fn request_open_history(&mut self, link: ViewId, history_id: u8, require_focus: bool) {
        self.deferred.push(Deferred::OpenHistory {
            link,
            history_id,
            require_focus,
        });
    }

    /// Request `recordHistory(link->data)` for the `THistory` broadcast arm —
    /// **deferred** ([`Deferred::RecordHistory`]). The pump resolves the link, reads
    /// its current text, and `history_add`s it to the channel.
    pub fn request_record_history(&mut self, link: ViewId, history_id: u8) {
        self.deferred
            .push(Deferred::RecordHistory { link, history_id });
    }

    /// Request a modal `messageBox` be opened from inside a downward-borrowed
    /// `&mut View` — **deferred** ([`Deferred::OpenMessageBox`]; the
    /// async-modal-from-a-view seam, `docs/design/async-modal-from-view.md`). A
    /// `valid()` (a validator `error`, the `FileEditor` modified-save prompt) holds
    /// only `&mut Context` and cannot run a nested modal inline, so it requests one;
    /// the pump builds + drives it.
    ///
    /// `answer_to = Some(id)` routes the chosen button [`Command`] back to that view
    /// via [`View::set_modal_answer`](crate::view::View::set_modal_answer), and
    /// `then_command = Some(cmd)` re-posts a focused command afterwards so the
    /// original action re-runs `valid()` with the cached answer. Both `None` for an
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

    /// Start mouse hold-tracking for `view` — the widget-facing D9 successor of
    /// entering the C++ `do { … } while (mouseEvent(event, mask))` blocking
    /// hold-loop (`tview.cpp:636-643`). Wraps [`push_capture`](Self::push_capture)
    /// with a [`MouseTrackCapture`](crate::capture::MouseTrackCapture): from the
    /// *next* pump on (the deferred-push latency, the `compose_full_protocol`
    /// invariant — matching the C++ `do{}while` running the body once before the
    /// first wait), every masked `MouseMove`/`MouseAuto`/wheel pseudo-down — and
    /// the terminating `MouseUp` — is localized against `origin` and delivered
    /// straight back to `view`'s `handle_event` via [`Deferred::MouseTrack`];
    /// everything else is swallowed (the hold is modal). The widget's own
    /// `MouseMove`/`MouseAuto` arms are the loop body; its `MouseUp` arm is the
    /// post-loop code.
    ///
    /// `origin` is the absolute screen position of `view`-local `(0, 0)`, cached
    /// by the widget's last `draw` (the `Button::abs_origin` /
    /// `ColorPicker::body_origin` pattern, D3/D9).
    ///
    /// The widget's `MouseUp` arm **must** be guarded by a `tracking` flag set
    /// at `MouseDown` time: `MouseUp` is not gated by `Group::wants`, so a
    /// stray, untracked up delivered via normal routing would otherwise reach
    /// the post-loop arm. See step 5 of `docs/design/mouse-track.md`.
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
    /// find_mut+handle_event delivery to forward a wheel pseudo-down to its
    /// sibling scrollbars (the C++ `vScrollBar->handleEvent(event)` /
    /// `hScrollBar->handleEvent(event)`, teditor1.cpp:574-579). Widgets enter
    /// tracking via [`start_mouse_track`](Self::start_mouse_track).
    pub(crate) fn request_mouse_track(&mut self, view: ViewId, event: Event) {
        self.deferred.push(Deferred::MouseTrack { view, event });
    }

    /// Request the pump to open a [`FileDialog`](crate::dialog::FileDialog) for
    /// `editor_id` to pick a save-as filename — **deferred**
    /// ([`Deferred::OpenSaveAsDialog`]). Called from
    /// `FileEditor::handle_event(cmSaveAs)` and `FileEditor::save()` when the buffer
    /// is untitled (`*fileName == EOS`). The pump builds + stashes the dialog; the
    /// `SaveAsPick` completion sets the picked name on the editor and re-injects
    /// `cmSave`.
    pub fn request_save_as_dialog(&mut self, editor_id: ViewId) {
        self.deferred.push(Deferred::OpenSaveAsDialog { editor_id });
    }

    /// Request the `TEditor` `editor` re-read its scrollbars' values and update its
    /// `delta` — **deferred** ([`Deferred::SyncEditorDelta`]). The editor (a leaf,
    /// D3) cannot read its window-frame sibling bars itself; the pump brokers the
    /// read (`checkScrollBar`). `h`/`v` are the bar [`ViewId`]s (`None` = no bar).
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
    /// **deferred** ([`Deferred::IndicatorSetValue`]). `TEditor::doUpdate` →
    /// `indicator->setValue`; the editor (a leaf, D3) cannot mutate its sibling
    /// indicator inline.
    pub fn set_indicator_value(&mut self, indicator: ViewId, location: Point, modified: bool) {
        self.deferred.push(Deferred::IndicatorSetValue {
            indicator,
            location,
            modified,
        });
    }

    /// Request `text` be copied to the system clipboard — **deferred**
    /// ([`Deferred::SetClipboard`]). `TEditor::clipCopy` → `TClipboard::setText`.
    pub fn set_clipboard(&mut self, text: String) {
        self.deferred.push(Deferred::SetClipboard(text));
    }

    /// Request the editor `id` paste the system-clipboard text — **deferred**
    /// ([`Deferred::EditorPaste`]). `TEditor::clipPaste` →
    /// `TClipboard::requestText`; the pump reads the clipboard and inserts.
    pub fn editor_paste(&mut self, id: ViewId) {
        self.deferred.push(Deferred::EditorPaste(id));
    }

    /// Request the `InputLine` `id` paste the system-clipboard text — **deferred**
    /// ([`Deferred::InputLinePaste`]). B3: `TInputLine::handleEvent cmPaste` →
    /// `TClipboard::requestText`; the pump reads the clipboard and calls
    /// [`InputLine::paste_text`](crate::widgets::InputLine::paste_text).
    pub fn request_input_line_paste(&mut self, id: ViewId) {
        self.deferred.push(Deferred::InputLinePaste(id));
    }

    /// Re-queue a **raw event** into the loop's event queue — the raw-event
    /// sibling of [`post`](Self::post) (which only ever queues an
    /// `Event::Command`). Ports `execute()`'s `putEvent(e)`
    /// (`tmnuview.cpp:375/405`): the menu session re-posts the triggering event so
    /// the next pump re-delivers it (e.g. an outside click that should reach the
    /// view recovering focus, or — stage 2 — a mouse event on submenu-open). Lands
    /// in `out_events`, drained before the backend is polled.
    pub fn put_event(&mut self, ev: Event) {
        self.out_events.push_back(ev);
    }

    /// The clock value sampled for this dispatch pass.
    pub fn now_ms(&self) -> u64 {
        self.now_ms
    }

    /// The owner's size for the view currently being routed to — the downward
    /// realization of `owner->size` / `owner->getExtent()` (D3). See the
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
    /// the C++ `owner->phase` read (D3: no up-pointer, so the phase rides the
    /// `Context`; see the [`phase`](Self::phase) field docs). Defaults to
    /// [`Phase::Focused`] outside a focused-events walk.
    pub fn phase(&self) -> Phase {
        self.phase
    }

    /// Set the dispatch phase for the routed view — called by
    /// `Group::route_event` before each leg of the focused-events walk
    /// (pre-process / focused / post-process, `tgroup.cpp:362-371`) and to
    /// restore the saved value on exit. Leaf views never call this — routing
    /// infrastructure only, hence `pub(crate)`.
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

    // -- cast_shadow (D8 shadow pass) -----------------------------------------

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
        // Context::new defaults phase to Focused (TGroup ctor, tgroup.cpp:28).
        assert_eq!(ctx.phase(), Phase::Focused);
        // The setter round-trips.
        ctx.set_phase(Phase::PostProcess);
        assert_eq!(ctx.phase(), Phase::PostProcess);
        ctx.set_phase(Phase::PreProcess);
        assert_eq!(ctx.phase(), Phase::PreProcess);
    }
}
