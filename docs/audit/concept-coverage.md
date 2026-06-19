# Concept coverage — Part 2 behavioral capabilities → mdBook

From the guide's Part 2 ("Using Turbo Vision", pp. 93–314): the *behavioral
possibilities* that aren't a single named symbol, plus every `→ concept` doc
route flagged during the Part 3 sweep. Input to a later mdBook pass. See
[`README.md`](README.md).

## Capability checklist

Legend for the mdBook column: a **chapter link** means the concept's *how-it-fits*
is explained there; **GAP** means the capability is present in code but the
mdBook never explains the mechanism (a conceptual documentation gap); **NOT-PORTED**
means a deliberately-dropped subsystem (the mdBook documents the drop, so no gap).

### Ch 7 — object model, coordinates, flag fields (pp. 93–112)

| Capability | Guide pp. | Present in port? (src evidence) | Covered in mdBook? (chapter / GAP) |
|---|---|---|---|
| Inheritance / object hierarchy (all views descend from a common base, override only what differs) | 93–99 | `View` trait + `ViewState` composition (`src/view/view.rs`, `src/view/mod.rs`); delegation via `#[delegate]` | `port/inheritance.md` (trait + composition) |
| Object typology (primitives / views / groups / non-view engines) | 101–107 | `Point`/`Rect` (`src/view/geometry.rs`), `View`/`Group` (`src/view/`), engines: validators/theme/collections-as-Vec | `port/inheritance.md`, `internals/view-tree.md` |
| Between-the-cells coordinate system, `i32` coords | 107–108 | `Rect`/`Point` with `i32` (`src/view/geometry.rs`) | `port/dropped.md` ("Coordinates stay `i32`") |
| Local vs global coordinates / make_local–make_global translation | 108–109 | Positional router subtracts child origin inline (`src/view/group.rs` `route_event`); **no named `make_local`/`make_global` public method** | **GAP** — coordinate translation happens implicitly in the router; no chapter explains local↔global for a custom view author |
| Bitmapped flag fields → bit ops | 109–112 | Flag words become struct-of-bools (`Options`/`GrowMode`/`DragMode`/`EventMask`) | `port/flags.md` |

### Ch 8 — views: lifecycle, draw-on-demand, state/options, drag, cursor, grow (pp. 113–147)

| Capability | Guide pp. | Present in port? (src evidence) | Covered in mdBook? (chapter / GAP) |
|---|---|---|---|
| Draw-on-demand (`Draw` must redraw fully at any time; content never persistent) | 114, 119–120 | `View::draw` contract; whole-tree redraw drives it (`src/view/`, `src/app/program.rs`) | `port/draw.md`, `internals/drawing.md`, `internals/custom-view.md` |
| Conditional redraw (`drawView` / draw-only-if-exposed) | 85–86 | Subsumed: whole-tree redraw + back-buffer diff (`src/screen/`, `src/app/program.rs`) | `port/draw.md`, `internals/drawing.md` (diff replaces conditional redraw) |
| Drawing through buffers (`TDrawBuffer`, MoveStr/WriteBuf) | 129–131 | `DrawBuffer` (`src/screen/draw_buffer.rs`) | `internals/drawing.md`, `internals/custom-view.md` |
| View lifecycle: init / insert / delete / dispose cascade | 116, 132, 146–147 | `View::init` (`src/view/view.rs`), `Group::insert`/recursive teardown (`src/view/group.rs`) | `internals/view-tree.md`, `internals/custom-view.md` |
| `awaken` post-load hook | 100 | `View::init` exists; **awaken is stream-load-only → NOT-PORTED with streams** | `port/dropped.md` (streaming dropped) |
| Option flags: selectable / topSelect / firstClick / framed / centered | 121 | `Options { selectable, top_select, first_click, framed, center_x/y }` (`src/view/view.rs`) | `port/flags.md`, `apps/windows.md` (tileable/options) |
| Option flags: pre-process / post-process | 121 | `Options { pre_process, post_process }` (`src/view/view.rs`); honoured by the phase router | `port/events.md`, `internals/event-loop.md` |
| State flags + `setState` change-response | 122–123 | `ViewState` flags + `View::set_state` propagation (`src/view/view.rs`, `src/view/group.rs`) | `port/flags.md`, `internals/view-tree.md`; **partial GAP** — "override setState to react to a state change" is not spelled out for custom-view authors |
| Drag limits (`DragMode` limit bits) | 124 | `DragMode { drag_move, drag_grow, drag_grow_left, … }` + size limits (`src/view/view.rs`, `src/window/window.rs`) | `apps/windows.md` (move/resize); **partial GAP** — limit-bit semantics not explained conceptually |
| Interactive drag move/resize (`dragView`) | 124–125 | Drag handler on the capture stack (`src/capture.rs`, `src/window/window.rs` move_grow) | `port/capture.md`, `internals/event-loop.md` |
| Cursor show/hide (`sfCursorVis`) | 125–126 | `ViewState::cursor_vis`, `show_cursor`/`hide_cursor` (`src/view/view.rs`) | `internals/event-loop.md`, `apps/text-editing.md` (cursor mention) |
| Cursor shape block vs underline (`sfCursorIns`) | 126 | `ViewState::cursor_ins` (`src/view/view.rs`) | **GAP** — block/underline (insert/typeover) cursor shape is present but no chapter explains it |
| Move cursor (`setCursor`) + placement needs absolute coords | 126 | `ViewState::set_cursor`; loop places hardware cursor (`src/view/view.rs`, `src/app/program.rs`) | `internals/event-loop.md` (resetCursor walk) |
| Grow modes (`gfGrowLoX/LoY/HiX/HiY/Rel`) — anchor edges to owner on resize | 141–142 | `GrowMode { grow_lo_x, …, grow_rel }`, `grow_all()` (`src/view/view.rs`); applied on `change_bounds` | `apps/windows.md` (grow mode override on splitters); **partial GAP** — the anchor-edges model isn't explained head-on |
| Z-order = reverse insertion order | 137–139 | Children Vec; back-to-front iteration (`src/view/group.rs`) | `port/handles.md`, `port/draw.md`, `internals/view-tree.md` |
| Z-order reordering (`putInFrontOf` / `makeFirst` / raise-on-select) | 117, 137–139 | Raise-to-top on select (`src/view/group.rs` ~605); `Desktop` arrange | `apps/windows.md` (window numbers/raise); **partial GAP** — no general "reorder a view in Z" primitive documented |
| Focus chain / single focused terminal view | 139–141 | Per-group current child + focus propagation (`src/view/group.rs`) | `internals/view-tree.md`, `internals/event-loop.md` |
| Group cache buffers (`ofBuffered`) + Redraw | 142–143 | **NOT-PORTED** — replaced by whole-tree redraw + diff | `port/draw.md`, `port/dropped.md` ("buffered drawing — dropped") |
| Lock/Unlock draw batching (flicker control) | 143 | **NOT-PORTED** — no lock/unlock; one diff at end of pump | `port/dropped.md`, `port/draw.md` ("you never call lock/unlock") |
| Subview clipping / `getClipRect` | 143–144 | Cells clipped at owner bounds in the buffer writer (`src/screen/`) | `internals/drawing.md`; **partial GAP** — clip-rect-driven partial draw isn't exposed/explained |

### Ch 9 — event-driven programming (pp. 149–169)

| Capability | Guide pp. | Present in port? (src evidence) | Covered in mdBook? (chapter / GAP) |
|---|---|---|---|
| Event packaging / kinds (mouse / keyboard / message / nothing) | 150–153 | `enum Event` + match (`src/event/mod.rs`) | `port/events.md` |
| Positional (mouse) event routing → topmost child under cursor | 154 | Positional hit-test in `route_event` (`src/view/group.rs`) | `port/events.md`, `internals/event-loop.md` |
| Focused event routing → focus chain, bubble-back-up | 154–155 | Focused leg of the three-phase router (`src/view/group.rs`) | `port/events.md`, `internals/event-loop.md` |
| Broadcast routing → all subviews in Z-order | 155–156 | Broadcast leg delivers to every child (`src/view/group.rs`) | `port/events.md`, `internals/brokering.md` |
| The Phase field (pre-process / focused / post-process) | 156–158 | `enum Phase` (`src/view/view.rs`), `ctx.phase()`/`set_phase` (`src/view/context.rs`), bracketed in router | `port/events.md`, `internals/event-loop.md`; **partial GAP** — the three legs are described but *why a view reads `phase()` and reacts differently* (Alt-letter in pre, plain letter in post) isn't drawn out |
| Event masking (`EventMask`, opt-in classes) | 156 | `struct EventMask` (`src/event/mod.rs`); `wants`/`blocked` gate (`src/view/group.rs`) | `port/events.md`, `port/flags.md`; **partial GAP** — opt-in of expensive classes (mouse-move/auto) covered in rustdoc but not narratively in a chapter |
| User-defined events + Positional/Focused routing masks | 156 | Closed `Event` enum (extension via new variants, not bitmask) | `port/events.md` (enum + match); routing-mask-for-new-events is N/A by design |
| Events → commands (status/menu generate `evCommand`) | 152–153, 159 | `Event::Command` from menu/status (`src/menu/`, `src/status/`) | `apps/commands.md`, `apps/menus.md` |
| Command definition + reserved ranges; only 0–255 disablable | 159 | `Command` open newtype (`src/command.rs`); `CommandSet` is a 256-bit set | `port/constants.md`, `apps/commands.md` |
| Command enable/disable + command sets (`TCommandSet`) | 79–82, 160 | `CommandSet` (`src/command.rs`), `ctx.enable_command`/`disable_command` (`src/view/context.rs`), pump snapshot (`src/app/program.rs`) | `apps/commands.md` |
| `cmCommandSetChanged` broadcast on change | — | Flagged + broadcast once on idle (`src/app/program.rs`) | `apps/commands.md` |
| `clearEvent` (mark handled, record who) | 152–153, 163 | `Event::consume`/nothing-state (`src/event/mod.rs`); handled-tracking via return | `port/events.md`; **partial GAP** — "who handled it" recording isn't surfaced as a concept |
| Abandoned events / `eventError` | 163 | Unhandled events fall through the pump (`src/app/program.rs`) | **GAP** — no chapter mentions the unhandled-event / eventError path |
| Idle-time processing (`TApplication::idle`) | 165, 193–194 | Idle pump pass (`src/app/program.rs` ~1720): status-line update, command-set-changed, mouse-auto synth | **GAP (biggest)** — there is **no user-facing `Idle`/`on_idle` hook** and no chapter explains how an app runs background work each idle pass (animation/clock/heap display). The idle pass exists but is internal-only |
| Inter-view messaging (`Message()` → who-handled pointer) | 156, 166–169 | `ctx.broadcast` (`src/view/context.rs`) + sibling brokering at deferred-apply (`src/view/group.rs`) | `internals/brokering.md`; **partial GAP** — broadcast-as-message and the "returned handler pointer / find topmost-of-type" probe idiom isn't documented |
| Override `getEvent` / inject event sources / keystroke macros | 163–165 | Single pump owns event acquisition (`src/app/program.rs`); backend trait feeds it (`src/backend/`) | **GAP** — no documented seam for injecting an extra event source or transforming the stream app-wide |
| Modality / scope of interaction (status line stays hot) | 144–145 | Capture stack + modal-frame gate (`src/capture.rs`); status-line pre-route (`src/app/program.rs`) | `port/modal.md`, `port/capture.md`, `internals/event-loop.md` |
| Execute a modal group / `execView` / `execDialog` (returns a command) | 145, 199–200 | `Deferred` push-capture / modal execution (`src/capture.rs`, `src/app/program.rs`) | `port/modal.md`, `apps/dialogs.md` |
| `topView` (current modal view) as broadcast target | 145 | Capture stack top resolves the modal scope (`src/capture.rs`) | `port/modal.md`, `port/capture.md` |
| End modal (`endModal(command)`) | 145–146 | `Deferred::EndModal` variant (`src/view/context.rs`, `src/app/program.rs`) | `port/modal.md`, `internals/deferred.md` |

### Ch 10 — application objects (pp. 171–194)

| Capability | Guide pp. | Present in port? (src evidence) | Covered in mdBook? (chapter / GAP) |
|---|---|---|---|
| Init/Run/Done lifecycle; app owns screen → menu/desktop/status | 172–174 | `Program`/`Application` (`src/app/program.rs`, `src/app/application.rs`); desktop/menu/status views | `getting-started/skeleton.md`, `internals/event-loop.md` |
| Desktop window management (insert/validate window) | 180 | `Desktop` (`src/desktop/desktop.rs`); `insert_child` pub(crate) | `apps/windows.md` |
| Window tiling / cascading (`ofTileable`) | 181–182 | `Desktop::tile`/`cascade` (`src/desktop/desktop.rs`) | `apps/windows.md` |
| Desktop background pattern (configurable / custom) | 182–184 | `Background` (`src/desktop/background.rs`) | `apps/windows.md` |
| Status line: clickable commands + hot keys, status defs | 185–188 | `StatusLine` + status defs (`src/status/`) | `apps/menus.md` |
| Context-dependent status line (def ranges by help context) | 186–189 | Help-context-ranged defs (`src/status/`, `src/help.rs`) | `apps/menus.md` ("first def whose range matches") |
| Context-sensitive hint text | 189, 191 | Hint hook in status line (`src/status/`) | `apps/menus.md` (partial); **partial GAP** — hint-by-context override not fully drawn out |
| Screen-mode switching (25/43/50-line) | 178–179 | **NOT-PORTED** — terminal-driven; no DOS video-mode toggle | (DOS-era; no analog) |
| Shell to DOS / suspend-resume | 185 | **NOT-PORTED** — DOS shell-out | (DOS-era; no analog) |
| Context-sensitive help (helpCtx per view → current context → help view) | 194 | `HelpCtx` (`src/help.rs`); focused-chain help context (`src/view/group.rs` ~896) | `apps/menus.md` ("Context-sensitive help") — note a help *viewer/window widget* is not a full ported widget |

### Ch 11 — window & dialog objects (pp. 195–209)

| Capability | Guide pp. | Present in port? (src evidence) | Covered in mdBook? (chapter / GAP) |
|---|---|---|---|
| Window = group/holder (insert subviews for content) | 195–196 | `Window` embeds `Group` (`src/window/window.rs`) | `apps/windows.md` |
| Dialog special event handling (Esc→cancel, Enter→default, auto-end on OK/Cancel/Yes/No) | 196, 205 | `Dialog` handle_event (`src/dialog/dialog.rs`) | `apps/dialogs.md` |
| Configurable move/grow/close/zoom (`WindowFlags`) | 197 | `WindowFlags` (`src/window/window.rs`) | `apps/windows.md` |
| Modal dialog execution returning a command | 199–200, 205 | Modal execution → terminating command (`src/app/program.rs`, `src/dialog/`) | `port/modal.md`, `apps/dialogs.md` |
| Whole-record data transfer (`getData`/`setData` round trip, skipped on cancel) | 200, 206–207 | D10 value protocol: `gather_data`/`scatter_data` (`src/view/group.rs`, `src/data.rs`) | `apps/dialogs.md` |
| Window palettes (blue/gray/cyan schemes) | 200–201 | `WindowPalette` → `Theme` roles (`src/window/`, `src/theme.rs`) | `apps/theming.md`, `apps/windows.md` |
| Window title via `getTitle` | 201 | Title field/override (`src/window/window.rs`, frame draws it) | `apps/windows.md` |
| Window numbering + Alt-N selection | 202 | Window number 1–9 → Alt-N (`src/window/window.rs`) | `apps/windows.md` |
| Size limits / min size; zoom toggle | 202–203 | `View::size_limits`, zoom rect (`src/window/window.rs`) | `apps/windows.md` |
| Standard scroll bars on frame (`standardScrollBar`) | 203–204 | `ScrollBar` + window helper (`src/widgets/scrollbar.rs`, `src/window/`) | `apps/windows.md`, `gallery.md` |
| Message boxes (formatted, flag-selected buttons → command) | 208–209 | `msgbox` (`src/dialog/msgbox.rs`) | `apps/dialogs.md` |
| Standard file / change-dir dialogs | 209 | `FileDialog` (`src/dialog/filedlg.rs`); change-dir dialog | `apps/dialogs.md`; **partial GAP** — change-directory dialog coverage thin |

### Ch 12 — control objects (pp. 211–235)

| Capability | Guide pp. | Present in port? (src evidence) | Covered in mdBook? (chapter / GAP) |
|---|---|---|---|
| Tab order = insertion/Z-order = getData/setData order | 206 | Insertion order drives tab + gather/scatter (`src/view/group.rs`) | `apps/dialogs.md`, `apps/controls.md`; **partial GAP** — the tab-order = transfer-order tie isn't stated |
| Per-control data transfer protocol (`dataSize`, typed records) | 213–215 | `FieldValue` per control + `View::value`/`set_value` (`src/data.rs`) | `apps/dialogs.md`, `apps/controls.md` |
| Static / parameterized text controls | 215–218 | `StaticText` (`src/widgets/static_text.rs`) | `apps/controls.md`, `gallery.md` |
| Scroll bar value model + `cmScrollBarChanged` broadcast | 219–220 | `ScrollBar` set_range/step/value + broadcast (`src/widgets/scrollbar.rs`) | `apps/controls.md`, `internals/brokering.md` |
| Cluster controls (check boxes / radio / multi-state, enableMask) | 221–223 | `Cluster` (`src/widgets/cluster.rs`) | `apps/controls.md`, `gallery.md` |
| List viewers / list boxes (`getText` override, `cmListItemSelected`) | 223–227 | `ListViewer`/`ListBox` (`src/widgets/list_viewer.rs`, `list_box.rs`) | `apps/controls.md`, `internals/brokering.md`, `gallery.md` |
| Outline viewers (tree, expand/contract) | 228–230 | `Outline` (`src/widgets/outline.rs`) | `gallery.md`; **partial GAP** — outline not in an apps chapter |
| Input line editing (edit, clipboard, h-scroll, selection) | 230–231 | `InputLine` (`src/widgets/input_line.rs`) | `apps/controls.md`, `gallery.md` |
| History lists (recall past inputs, shared by history id) | 231–233 | `History`/`HistoryViewer` with `history_id` channels (`src/widgets/history.rs`) | `apps/controls.md`; **partial GAP** — persistence (storeHistory/loadHistory) idiom not documented |
| Label controls with focus-linking | 233–235 | `Label` (`src/widgets/`); `cmReceivedFocus` linking | `apps/controls.md` |

### Ch 13 — data validation (pp. 237–245)

| Capability | Guide pp. | Present in port? (src evidence) | Covered in mdBook? (chapter / GAP) |
|---|---|---|---|
| Three validation kinds: filter input / validate field / validate screen | 237–238 | `Validator` trait: `is_valid_input` (filter), `is_valid`/`validate` (`src/validate.rs`) | `apps/controls.md` |
| Filtering (per-keystroke restriction) | 237–238 | `Validator::is_valid_input` (`src/validate.rs`); InputLine consults it | `apps/controls.md` |
| Validate-on-focus-change (`ofValidate` on field/window) | 238–239 | `Options::validate` + focus-release gate (`src/view/group.rs` ~2028 "validating invalid current blocks focus release") | `apps/controls.md`; **partial GAP** — the ofValidate focus-hold behaviour isn't spelled out |
| Validate modal window on close (unless cancel) | 239 | Dialog `valid(cmClose)` walk before end-modal (`src/dialog/dialog.rs`) | `apps/dialogs.md`; **partial GAP** |
| Validate-on-demand (`Valid(cmClose)` without closing) | 239 | `View::valid` group walk (`src/view/group.rs`) | **partial GAP** — on-demand valid() is in code but not called out |
| Validator method protocol (Valid/IsValid/IsValidInput/Error) | 241–242 | `Validator` trait methods (`src/validate.rs`) | `apps/controls.md` |
| Standard validators (filter/range/lookup/string-lookup/picture) + RegexValidator | 243–245 | `FilterValidator`/`RangeValidator`/`LookupValidator`/`StringLookupValidator`/`PXPictureValidator`/`RegexValidator` (`src/validate.rs`) | `apps/controls.md` (full table) |

### Ch 14 — palettes & color (pp. 247–257)

| Capability | Guide pp. | Present in port? (src evidence) | Covered in mdBook? (chapter / GAP) |
|---|---|---|---|
| Palette/color-chain mapping (view index → owner palette → app palette) | 247–251 | **EQUIVALENT (D7)** — owner-chain traversal replaced by `Role → Style` map (`src/theme.rs`) | `apps/theming.md`, `port/theme.md` (heritage note explains the replacement) |
| App palette holds only real attributes (color/BW/mono triple) | 249–252 | `Theme` is the single swappable table; `Style` carries RGB (`src/theme.rs`, `src/color.rs`) | `apps/theming.md` |
| Per-view palette override / palette extension | 252–255 | A widget chooses its `Role`; new roles are closed-enum (first-party only) | `apps/theming.md`, `internals/custom-view.md`; **partial GAP** — "how to give a custom view its own colors" via Role isn't a full recipe |

### Ch 15 — editor & text objects (pp. 259–274)

| Capability | Guide pp. | Present in port? (src evidence) | Covered in mdBook? (chapter / GAP) |
|---|---|---|---|
| Editor text buffer (gap buffer) | 263–266 | `Editor` buffer with gap (`src/widgets/editor.rs`) | `apps/text-editing.md` |
| Editor undo (single-level, since last cursor move) | 264–266 | `Editor::undo`, ins/del counters (`src/widgets/editor.rs` ~1258) | `apps/text-editing.md` ("single-level undo") |
| Editor selection / block handling | 266 | sel-start/end + insert-replaces-selection (`src/widgets/editor.rs`) | `apps/text-editing.md` |
| Clipboard cut/copy/paste via shared clipboard editor | 266, 273–274 | `is_clipboard` editor + cut/copy/paste brokers (`src/widgets/editor.rs`, `src/backend/clipboard.rs`) | `apps/text-editing.md`; OS clipboard chain (`src/backend/clipboard.rs`) |
| UpdateCommands (enable/disable edit commands by state) | 267 | Editor updates cut/copy/paste enable on state (`src/widgets/editor.rs` ~1475) | **partial GAP** — command-self-gating of the editor isn't documented |
| WordStar key bindings / block simulation | 267 | Editor key handling (`src/widgets/editor.rs`) | `apps/text-editing.md` (word-by-word) ; **partial GAP** — WordStar/Ctrl-K bindings not enumerated |
| Search and replace | 267–268 | Editor search/replace path (`src/widgets/editor.rs`) | **partial GAP** — find/replace flow not documented |
| File editor load/save (+ backup) | 270–272 | `FileEditor::new_file_editor`, save/save-as, `EF_BACKUP_FILES` (`src/widgets/editor.rs`) | `apps/text-editing.md` (FileEditor, backup `~`) |
| Save-on-close guard (modified prompt) | 271 | `valid(cmClose)` prompt on modified file editor (`src/widgets/editor.rs`) | `apps/text-editing.md` (partial) |
| Memo control (editor as dialog control, traps Tab, getData/setData) | 268–269 | Editor-as-control wiring (`src/widgets/editor.rs`, `src/data.rs`) | **partial GAP** — Memo-as-control usage thin |
| Edit window (titles from file, hides instead of closes for clipboard) | 274–275 | Edit-window behaviour (`src/widgets/editor.rs`, `src/window/`) | `apps/text-editing.md` (partial) |
| Terminal / text device (scrollable write-only, circular buffer) | 259–262 | `Terminal` + `TextDevice` (`src/widgets/terminal.rs`, ports `TTerminal`/`TTextDevice`) | `gallery.md`; **partial GAP** — terminal not in an apps chapter |

### Ch 16–18 — collections / streams / resources (pp. 277–314) — dropped subsystems

| Capability | Guide pp. | Present in port? (src evidence) | Covered in mdBook? (chapter / GAP) |
|---|---|---|---|
| Collections subsystem (`TCollection`/`TSortedCollection`/`TStringCollection`) | 277–289 | **NOT-PORTED** — supplanted by `Vec`/`BTreeSet` + closures | `reference/deviations.md`, `reference/symbol-map.md` (TCollection → Vec) |
| Streams subsystem (`TStreamable`, put/get, type registry, view-tree persistence) | 291–308 | **NOT-PORTED** — dropped wholesale (serde if ever revived) | `port/dropped.md` ("Streaming & persistence — dropped") |
| Resources subsystem (`TResourceFile`, keyed object store) | 309–314 | **NOT-PORTED** — depends on streams; dropped | `port/dropped.md` |
| String list resources (`TStringList`, localizable strings) | 313–314 | **NOT-PORTED** — part of dropped resources | `port/dropped.md` |

## `→ concept` routes from the per-symbol reference

Per-symbol audit rows that flagged the doc gap as conceptual (belongs in the mdBook narrative, not a longer rustdoc comment):

| Section | Entry | Why it's a concept gap |
|---|---|---|
| TGroup | `Phase` (field) | C++ `Phase: (phFocused, phPreProcess, phPostProcess)` read by subviews as `Owner^.Phase`. tvision-rs carries the phase on the shared `Context` (set by the router around each leg, save/restored unde… |
| TGroup | `EndModal` (method) | C++ `EndModal(cmd)`: terminate the current modal view's modal state, returning `cmd` from `ExecView`. tvision-rs has no nested modal loop on the group; a view requests modal end via the deferred `E… |
| TGroup | `ExecView` (method) | C++ `ExecView(P)`: save context, set `sfModal`, insert P, `Execute`, restore, return result (`cmCancel` if P nil). tvision-rs `exec_view` pushes a [`ModalFrame`] onto the capture stack and runs the… |
| TGroup | `Execute` (method) | C++ `Execute`: the group's own `repeat GetEvent/HandleEvent until Valid(EndState)` modal loop. tvision-rs has ONE event loop in `Program` (`run` = `while end_state.is_none() { pump_once() }`); a gr… |
| TStringLookupValidator | `Error` (method) | C++ `error` calls `MessageBox` to display "not in list" dialog. Rust `error` calls `ctx.request_message_box("Input is not in list of valid strings", Error, ok-only, None, None)` — the async-modal-f… |
| TView | `DrawView` (method) | C++ `DrawView` = "draw only if `Exposed`". tvision-rs drops per-view exposure: the loop redraws the whole tree each pump and diffs against the prior buffer (D11/D-redraw). No `draw_view` method — `… |
