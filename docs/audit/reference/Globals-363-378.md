# Globals / Procedures / Constants (guide pp. 363–378)

Rust module(s): `src/view/view.rs`, `src/view/group.rs`, `src/view/context.rs`,
`src/widgets/scrollbar.rs`, `src/backend/traits.rs`, `src/backend/crossterm_backend.rs`,
`src/theme.rs`, `src/app/program.rs`, `src/status/mod.rs`, `src/menu/mod.rs`,
`src/dialog/msgbox.rs`, `src/widgets/history.rs`, `src/widgets/editor.rs`
| magiblot: `tview.cpp`, `tvwrite.cpp`, `tscrolba.cpp`, `system.h`, `drivers.cpp`,
`tprogram.cpp`, `tstatusl.cpp`, `menus.h`, `app.cpp`, `editors.cpp`, `histlist.cpp`

> **Scope:** `ovXXXX` constants (continued from p. 363), `PositionalEvents`,
> `PrintStr`, `PtrRec`, `RegisterColorSel`, `RegisterDialogs`, `RegisterEditors`,
> `RegisterStdDlg`, `RegisterType`, `RegisterValidate`, `RepeatDelay`, `ReplaceStr`,
> `SaveCtrlBreak`, `sbXXXX` constants (scrollbar parts + orientation), `ScreenBuffer`,
> `ScreenHeight`, `ScreenMode`, `ScreenWidth`, `SelectMode` type, `SetBufferSize`,
> `SetMemTop`, `SetVideoMode`, `sfXXXX` constants (state flags), `ShadowAttr`,
> `ShadowSize`, `ShowMarkers`, `ShowMouse`, `smXXXX` constants, `SpecialChars`,
> `stXXXX` constants, `StartupMode`, `StatusLine` variable, `StdEditMenuItems`,
> `StdEditorDialog`, `StdFileMenuItems`, `StdStatusKeys`, `StdWindowMenuItems`,
> `StreamError`, `StoreHistory`, `StoreIndexes`, `SysColorAttr`, `SysErrActive`,
> `SysErrorFunc`, `SysMonoAttr`, `SystemError`.

---

## ovXXXX constants — outline view flags (p. 363)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `ovExpanded` ($01) | 363 | PORTED | OK | `const OV_EXPANDED: u16 = 0x01` (`src/widgets/outline.rs:66`, private) | N/A | Private constant — not public API. Has a one-line comment explaining the flag's role. Scored N/A per visibility policy. |
| `ovChildren` ($02) | 363 | PORTED | OK | `const OV_CHILDREN: u16 = 0x02` (`src/widgets/outline.rs:68`, private) | N/A | Private constant — not public API. Has a one-line comment. Scored N/A per visibility policy. |
| `ovLast` ($04) | 363 | PORTED | OK | `const OV_LAST: u16 = 0x04` (`src/widgets/outline.rs:70`, private) | N/A | Private constant — not public API. Has a one-line comment. Scored N/A per visibility policy. |

## PositionalEvents variable (p. 364)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `PositionalEvents` | 364 | EQUIVALENT | OK | `EventMask { mouse_move, mouse_auto }` struct (`src/event/mod.rs:190`) + mouse-event routing in `Group::handle_event` (`src/view/group.rs:1194`) | 2 | C++: global `Word = evMouse` used by `TGroup::HandleEvent` to classify events. Rust: `Group` routes mouse events positionally, focused events by focus chain — the logic is inlined in `group.rs` dispatch rather than a mutable global. Functionally equivalent. `EventMask` has no `positional_events()` function. |

## PrintStr procedure (p. 364)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `PrintStr` | 364 | NOT-PORTED | — | — | — | DOS-specific: calls DOS function 40H to write to stdout without linking the I/O runtime. No analog needed — Rust's `print!`/`stdout().write_all()` are fully portable. |

## PtrRec type (p. 365)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `PtrRec` | 365 | NOT-PORTED | — | — | — | DOS 16-bit far-pointer record (`Ofs`, `Seg`). No analog on a flat-address Rust target; not needed. |

## RegisterColorSel procedure (p. 365)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `RegisterColorSel` | 365 | NOT-PORTED | — | — | — | `TStreamable` registration for `ColorSel` unit types. `TStreamable` is dropped (serde-if-revived per design). |

## RegisterDialogs procedure (p. 365)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `RegisterDialogs` | 365 | NOT-PORTED | — | — | — | `TStreamable` stream registration for `TDialog`, `TInputLine`, `TButton`, `TCluster`, `TRadioButtons`, `TCheckBoxes`, `TListBox`, `TStaticText`, `TParamText`, `TLabel`, `THistory`. `TStreamable` dropped entirely. |

## RegisterEditors procedure (p. 365)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `RegisterEditors` | 365 | NOT-PORTED | — | — | — | `TStreamable` stream registration for `TEditor`, `TMemo`, `TFileEditor`, `TIndicator`, `TEditWindow`. `TStreamable` dropped entirely. |

## RegisterStdDlg procedure (p. 366)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `RegisterStdDlg` | 366 | NOT-PORTED | — | — | — | `TStreamable` stream registration for `TFileInputLine`, `TFileCollection`, `TFileList`, `TFileInfoPane`, `TFileDialog`, `TDirCollection`, `TDirListBox`, `TChDirDialog`. `TStreamable` dropped entirely. |

## RegisterType procedure (p. 366)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `RegisterType` | 366 | NOT-PORTED | — | — | — | Core `TStreamable` registration: inserts a `TStreamRec` into the known-types list so `TStream.Get`/`TStream.Put` can deserialize by type tag. Entire stream/streamable system dropped. |

## RegisterValidate procedure (p. 366)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `RegisterValidate` | 366 | NOT-PORTED | — | — | — | `TStreamable` stream registration for `TPXPictureValidator`, `TFilterValidator`, `TRangeValidator`, `TLookupValidator`, `TStringLookupValidator`. `TStreamable` dropped entirely. |

## RepeatDelay variable (p. 366)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `RepeatDelay` | 366 | NOT-PORTED | — | — | — | DOS-specific: clock-tick count before `evMouseAuto` repeats start. Rust event synthesis (`src/timer.rs`) uses `std::time::Duration`-based timing rather than a mutable global tick counter. Intentionally dropped (DOS driver artifact). |

## ReplaceStr variable (p. 367)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `ReplaceStr` | 367 | EQUIVALENT | OK | `Editor::replace_str` (private field; `pub(crate)` accessors `replace_str()` / `set_replace_str()`) | N/A | C++: global `string[80]` shared across all editors. Rust: private per-instance field, accessed via `pub(crate)` methods — not part of the public API. Scored N/A per visibility policy (pub(crate) accessor). |

## SaveCtrlBreak variable (p. 367)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `SaveCtrlBreak` | 367 | NOT-PORTED | — | — | — | DOS-specific: saves the DOS Ctrl+Break state before `InitSysError` disables it. No Ctrl+Break intercept on modern terminals; dropped with the DOS driver layer. |

## sbXXXX constants — scrollbar parts (p. 367–368)

The C++ `sb*` family has two groups: (1) mouse-hit part identifiers used by `ScrollStep`; (2) `TWindow.StandardScrollBar` orientation/option flags.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `sbLeftArrow` (0) | 367 | EQUIVALENT | OK | `Part::LeftArrow` (`src/widgets/scrollbar.rs:60`, private enum) | N/A | Private enum variant — not public API. Has an inline comment. Scored N/A per visibility policy. |
| `sbRightArrow` (1) | 367 | EQUIVALENT | OK | `Part::RightArrow` (`src/widgets/scrollbar.rs:62`, private enum) | N/A | Private enum variant — not public API. |
| `sbPageLeft` (2) | 367 | EQUIVALENT | OK | `Part::PageLeft` (`src/widgets/scrollbar.rs:64`, private enum) | N/A | Private enum variant — not public API. |
| `sbPageRight` (3) | 367 | EQUIVALENT | OK | `Part::PageRight` (`src/widgets/scrollbar.rs:66`, private enum) | N/A | Private enum variant — not public API. |
| `sbUpArrow` (4) | 367 | EQUIVALENT | OK | `Part::UpArrow` (`src/widgets/scrollbar.rs:68`, private enum) | N/A | Private enum variant — not public API. |
| `sbDownArrow` (5) | 367 | EQUIVALENT | OK | `Part::DownArrow` (`src/widgets/scrollbar.rs:70`, private enum) | N/A | Private enum variant — not public API. |
| `sbPageUp` (6) | 367 | EQUIVALENT | OK | `Part::PageUp` (`src/widgets/scrollbar.rs:72`, private enum) | N/A | Private enum variant — not public API. |
| `sbPageDown` (7) | 367 | EQUIVALENT | OK | `Part::PageDown` (`src/widgets/scrollbar.rs:74`, private enum) | N/A | Private enum variant — not public API. |
| `sbIndicator` (8) | 367 | EQUIVALENT | OK | `Part::Indicator` (`src/widgets/scrollbar.rs:76`, private enum) | N/A | Private enum variant — triggers thumb-drag rather than scroll-step. Not public API. |
| `sbHorizontal` ($0000) | 368 | EQUIVALENT | OK | `ScrollBar` orientation inferred from bounds (`size.y == 1` → horizontal) | N/A | Not a distinct Rust constant — orientation is implicit in the rect at construction. No public symbol to score. |
| `sbVertical` ($0001) | 368 | EQUIVALENT | OK | `ScrollBar` orientation inferred from bounds (`size.x == 1` → vertical) | N/A | Same as `sbHorizontal` above — no public constant exists. |
| `sbHandleKeyboard` ($0002) | 368 | EQUIVALENT | OK | `tv::scrollbar::ScrollBar::with_keyboard()` / `Window::standard_scroll_bar(handle_keyboard: true)` | 2 | C++: flag enabling keyboard commands. Rust: opt-in builder method sets `ofPostProcess`. `with_keyboard()` is in `src/widgets/scrollbar.rs` (not in this pass's permitted files). |

## ScreenBuffer variable (p. 368)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `ScreenBuffer` | 368 | NOT-PORTED | — | — | — | DOS-specific: pointer to the video RAM buffer set by `InitVideo`. Rust uses the crossterm backend's own internal cell buffer (`src/backend/renderer.rs`); no direct video-RAM pointer is exposed. |

## ScreenHeight variable (p. 368)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `ScreenHeight` | 368 | EQUIVALENT | OK | `Backend::size() -> (u16, u16)` (second element) / `Program::desktop_size().y` | 1 | C++: global `Byte` set by `InitVideo`. Rust: queried on demand via `backend.size()` (no mutable global). Program stores live size in the root group's bounds. Doc score 1 (no usage guidance). |

## ScreenMode variable (p. 368–369)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `ScreenMode` | 368 | NOT-PORTED | — | — | — | DOS video mode word (CO80, BW80, Mono, etc.) changed via `SetVideoMode`. No analog: crossterm handles terminal mode detection. See also `smXXXX` below. |

## ScreenWidth variable (p. 369)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `ScreenWidth` | 369 | EQUIVALENT | OK | `Backend::size() -> (u16, u16)` (first element) / `Program::desktop_size().x` | 1 | C++: global `Byte` set by `InitVideo`. Same as `ScreenHeight` — dynamic query, no global. |

## SelectMode type (p. 369)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `SelectMode` (`NormalSelect`, `EnterSelect`, `LeaveSelect`) | 369 | EQUIVALENT | OK | `tv::view::SelectMode` enum (`Normal`, `Enter`, `Leave`) | 3 | C++: Pascal enum used by `TGroup.ExecView` and `TGroup.SetCurrent`. Rust: identical semantics, Rust enum (D5). Doc now adds how/when: pass `Normal` for all ordinary focus changes; `Enter`/`Leave` are modal-loop plumbing in `Program::exec_view` only. |

## SetBufferSize function (p. 369)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `SetBufferSize` | 369 | NOT-PORTED | — | — | — | DOS-specific: resizes a buffer heap allocation made by `NewBuffer`. Rust uses `Vec` for dynamic buffers; no fixed-size buffer heap. |

## SetMemTop procedure (p. 369)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `SetMemTop` | 369 | NOT-PORTED | — | — | — | DOS-specific: sets the top of the application's heap block (for DOS shell). No analog in Rust. |

## SetVideoMode procedure (p. 370)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `SetVideoMode` | 370 | NOT-PORTED | — | — | — | DOS EGA/VGA mode switching (CO80/BW80/Mono + smFont8x8). crossterm handles terminal mode; no direct video-mode API exposed. |

## sfXXXX constants — state flags (pp. 370–371)

C++ `sfXXXX` are bits in `TView.State`. Rust replaces the flag word with `tv::view::State` (struct-of-bools) + a narrow `tv::view::StateFlag` enum for the four group-propagated flags (D5). The dropped flag is `sfExposed` (see note).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `sfVisible` ($0001) | 370 | EQUIVALENT | OK | `tv::view::State::visible: bool` | 3 | Guide: set by default; `Show`/`Hide` modify it. Rust: `ViewState::show()`/`hide()`. `State` struct doc now adds how/when to read each flag (focused → draw highlight; cursor_vis/cursor_ins → set in handle_event; disabled → skip event handling). |
| `sfCursorVis` ($0002) | 370 | EQUIVALENT | OK | `tv::view::State::cursor_vis: bool` | 3 | Guide: hardware cursor visible while focused. Set in `handle_event` to show the cursor after text input. |
| `sfCursorIns` ($0004) | 370 | EQUIVALENT | OK | `tv::view::State::cursor_ins: bool` | 3 | Guide: cursor is solid block (insert mode). Toggle in `handle_event` on the Ins key. |
| `sfShadow` ($0008) | 370 | EQUIVALENT | OK | `tv::view::State::shadow: bool` | 3 | Guide: view casts a drop shadow. Set at construction (e.g. `Window` sets it by default). |
| `sfActive` ($0010) | 370 | EQUIVALENT | OK | `tv::view::State::active: bool` + `tv::view::StateFlag::Active` | 3 | Guide: view is in the active window chain. Set by framework via `Group::set_state`; read in draw to choose active/inactive appearance. |
| `sfSelected` ($0020) | 370 | EQUIVALENT | OK | `tv::view::State::selected: bool` + `tv::view::StateFlag::Selected` | 3 | Guide: view is the current subview. Set by `Group::set_current`; read in draw to show the selected (current) indicator. |
| `sfFocused` ($0040) | 370 | EQUIVALENT | OK | `tv::view::State::focused: bool` + `tv::view::StateFlag::Focused` | 3 | Guide: selected AND whole owner chain active. The primary flag to check in `draw` for the focused appearance (highlight bar, cursor, etc.). |
| `sfDragging` ($0080) | 370 | EQUIVALENT | OK | `tv::view::State::dragging: bool` + `tv::view::StateFlag::Dragging` | 3 | Guide: view is being dragged/resized. Set by the drag handler; read in draw if the view needs a distinct drag appearance. |
| `sfDisabled` ($0100) | 371 | EQUIVALENT | OK | `tv::view::State::disabled: bool` | 3 | Guide: view ignores all events. The framework gates events before they reach a disabled view; read in draw to show the disabled (greyed) appearance. |
| `sfModal` ($0200) | 371 | EQUIVALENT | OK | `tv::view::State::modal: bool` | 3 | Guide: view runs a modal event loop. Set by `Program::exec_view`; read by `Group::is_valid` to determine the active modal boundary. |
| `sfExposed` ($0800) | 371 | NOT-PORTED | — | — | — | Guide: view is owned/indirectly visible by `Application`. Dropped — whole-tree redraw (D9) makes per-view exposure tracking unnecessary. Module doc (`view.rs:76`) explicitly notes the drop. |

## ShadowAttr variable (p. 371)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `ShadowAttr` | 371 | EQUIVALENT | OK | `tv::theme::Role::Shadow` style in `Theme::classic_blue()` (dark gray on black, `0x8`/`0x0`) | 2 | C++: global `Byte = $08` (dark gray). Rust: the shadow colour is a `Theme` entry keyed by `Role::Shadow`; `classic_blue()` sets it to `0x8` on `0x0`. Mutable-global replaced by per-program `Theme`. Doc score 2 (what, not how to customize). |

## ShadowSize variable (p. 371–372)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `ShadowSize` | 371 | EQUIVALENT | OK | `tv::view::SHADOW_SIZE: Point = Point::new(2, 1)` (`src/view/context.rs:583`) | 3 | C++: global `TPoint = (X: 2; Y: 1)`. Rust: `pub const SHADOW_SIZE` in `DrawCtx`'s context module. Doc now explains who reads it (`DrawCtx` during draw), that it is compile-time-fixed (unlike mutable C++ global), and how to work around it if a custom offset is needed. |

## ShowMarkers variable (p. 372)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `ShowMarkers` | 372 | NOT-PORTED | — | — | — | Controls whether monochrome focus indicator characters (`SpecialChars`) appear around focused controls. Monochrome-mode-specific feature not yet ported; no `SpecialChars` rendering pass exists. → gap-report candidate. |

## ShowMouse procedure (p. 372)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `ShowMouse` | 372 | NOT-PORTED | — | — | — | DOS mouse driver: decrements hide counter, shows cursor when zero. Crossterm does not expose a show/hide mouse cursor API this way; mouse visibility is handled by the terminal itself. Intentional drop. |

## smXXXX constants — screen modes (p. 372)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `smBW80` ($0002) | 372 | NOT-PORTED | — | — | — | DOS black-and-white 80-column mode. No crossterm analog. |
| `smCO80` ($0003) | 372 | NOT-PORTED | — | — | — | DOS color 80-column mode. No crossterm analog. |
| `smMono` ($0007) | 372 | NOT-PORTED | — | — | — | DOS monochrome mode. No crossterm analog. |
| `smFont8x8` ($0100) | 372 | NOT-PORTED | — | — | — | DOS EGA/VGA 43/50-line mode selector. No crossterm analog. |

## SpecialChars variable (p. 373)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `SpecialChars` | 373 | NOT-PORTED | — | — | — | Array of 6 chars (`#175 #174 #26 #27 ' ' ' '`) drawn around the focused view in monochrome mode (controlled by `ShowMarkers`). No monochrome marker pass exists in the Rust port. Linked to `ShowMarkers` gap. |

## stXXXX constants — stream access modes and error codes (p. 373)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `stCreate` ($3C00) | 373 | NOT-PORTED | — | — | — | `TDosStream` / `TBufStream` file-open mode. Stream system dropped. |
| `stOpenRead` ($3D00) | 373 | NOT-PORTED | — | — | — | Same. |
| `stOpenWrite` ($3D01) | 373 | NOT-PORTED | — | — | — | Same. |
| `stOpen` ($3D02) | 373 | NOT-PORTED | — | — | — | Same. |
| `stOk` (0) | 373 | NOT-PORTED | — | — | — | Stream error code. Stream system dropped. |
| `stError` (-1) | 373 | NOT-PORTED | — | — | — | Same. |
| `stInitError` (-2) | 373 | NOT-PORTED | — | — | — | Same. |
| `stReadError` (-3) | 373 | NOT-PORTED | — | — | — | Same. |
| `stWriteError` (-4) | 373 | NOT-PORTED | — | — | — | Same. |
| `stGetError` (-5) | 373 | NOT-PORTED | — | — | — | Same. |
| `stPutError` (-6) | 373 | NOT-PORTED | — | — | — | Same. |

## StartupMode variable (p. 373–374)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `StartupMode` | 373 | NOT-PORTED | — | — | — | DOS: stores pre-initialization screen mode so `DoneVideo` can restore it. No analog — crossterm handles terminal state save/restore on `AlternateScreen` entry/exit. |

## StatusLine variable (p. 374)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `StatusLine` | 374 | EQUIVALENT | OK | `tv::status::StatusLine` struct + `Program::status_line() -> Option<ViewId>` | 2 | C++: global `PStatusLine = nil` pointer set by `TProgram.InitStatusLine`. Rust: `Program` holds the status line as a child view looked up by `ViewId`; no global mutable pointer. `Program::status_line()` returns the optional id. |

## StdEditMenuItems function (p. 374)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `StdEditMenuItems` | 374 | EQUIVALENT | OK | `Menu::builder()` with `Undo`/`Cut`/`Copy`/`Paste`/`Clear` items (user-side builder pattern) | 1 | C++: returns a linked list of `TMenuItem` for the standard Edit menu. Rust: no single `std_edit_menu_items()` free function; the test `builder_reproduces_file_window_menu` shows the idiomatic `MenuBuilder` chain. App implementors build the equivalent items inline. MISSING if a standalone free function is required; currently provided only via builder API. |

## StdEditorDialog function (p. 374)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `StdEditorDialog` | 374 | EQUIVALENT | OK | `Program::open_file_dialog` + modal find/replace dialogs driven via `FindPick` / `ReplacePick` `ModalCompletion` variants | 2 | C++: dispatches editor-specific dialogs (find, replace, open file) by `Dialog` integer code. Rust: each dialog type is a separate top-level path in the pump's `ModalCompletion` handler in `program.rs`. Idiomatic — type-safe dispatch replaces opaque integer code. |

## StdFileMenuItems function (p. 374–375)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `StdFileMenuItems` | 374 | EQUIVALENT | OK | `Menu::builder()` with New/Open/Save/SaveAs/SaveAll/ChangeDir/DosShell/Exit items (user-side builder) | 1 | Same pattern as `StdEditMenuItems`. No free function — app builds via `MenuBuilder`. |

## StdStatusKeys function (p. 375)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `StdStatusKeys` | 375 | EQUIVALENT | OK | `tv::status::StatusDef::list()` builder; default keys (Alt-X→cmQuit, F10→cmMenu, F1→cmHelp) shown in `builder_reproduces_default_status_line` test | 2 | C++: free function returning a standard `TStatusItem` linked list. Rust: fluent `StatusDef::list()` builder; the default status line is assembled by the app (example in test). The C++ key set (Alt-X, F10, Alt-F3, F5, Ctrl-F5, F6) is representable; no single free function provides them pre-built. |

## StdWindowMenuItems function (p. 375)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `StdWindowMenuItems` | 375 | EQUIVALENT | OK | `Menu::builder()` with Tile/Cascade/CloseAll/Size-Move/Zoom/Next/Prev/Close items (user-side builder) | 1 | Same pattern as `StdFileMenuItems`/`StdEditMenuItems`. |

## StreamError variable (p. 376)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `StreamError` | 376 | NOT-PORTED | — | — | — | Global procedure pointer called when a `TStream.Error` occurs. Stream system dropped entirely. |

## StoreHistory procedure (p. 376)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `StoreHistory` | 376 | NOT-PORTED | — | — | — | Writes the history block to a `TStream`. `TStreamable` dropped; history data is in-process only (`src/widgets/history.rs` global `Vec`). If persistence is needed, the caller serializes via serde. |

## StoreIndexes procedure (p. 376)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `StoreIndexes` | 376 | NOT-PORTED | — | — | — | Writes the `ColorIndexes` variable to a `TStream`. Color-sel stream persistence not ported (`TStreamable` dropped). |

## SysColorAttr variable (p. 376–377)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `SysColorAttr` | 376 | NOT-PORTED | — | — | — | DOS system error handler: default color attribute ($4E4F) for error messages on color systems. DOS sys-error handler not ported (DOS interrupt 09H/24H machinery). |

## SysErrActive variable (p. 377)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `SysErrActive` | 377 | NOT-PORTED | — | — | — | Boolean flag: system error manager is currently active (set by `InitSysError`). DOS interrupt handler; not ported. |

## SysErrorFunc variable (p. 377)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `SysErrorFunc` | 377 | NOT-PORTED | — | — | — | Pointer to the active system error function (default `SystemError`). DOS critical-error / disk-swap handler; not ported. |

## SysMonoAttr variable (p. 377)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `SysMonoAttr` | 377 | NOT-PORTED | — | — | — | DOS system error handler: default monochrome attribute ($7070) used in place of `SysColorAttr` on mono systems. Not ported. |

## SystemError function (p. 378)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `SystemError` | 378 | NOT-PORTED | — | — | — | Default DOS critical error / disk-swap handler: displays one of 16 error messages on the status line. DOS interrupt 09H/1BH/21H/23H/24H machinery; not ported. |

---

## Summary

- PORTED: 3   EQUIVALENT: 35   NOT-PORTED: 43   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 5   |   → concept: 0
- Pass 1 raised to score 3 (in permitted files `src/view/view.rs`, `src/view/group.rs`, `src/view/context.rs`): all `sfXXXX` state flag fields in `State`, `SelectMode`, `SHADOW_SIZE`. Reclassified as N/A-private: `ovXXXX` constants (private in `outline.rs`), `sbXXXX` scrollbar parts (private enum variants + no-symbol orientation), `ReplaceStr`→`replace_str` (pub(crate) accessor). Remaining doc<3 (public, out-of-scope files): `ScreenHeight`/`ScreenWidth` (score 1, `Backend::size()` in `src/backend/traits.rs`), `StatusLine` (score 2, `src/status/mod.rs`), `StdEditorDialog` (score 2, `src/app/program.rs`), `StdStatusKeys` (score 2), `sbHandleKeyboard` (score 2, `src/widgets/scrollbar.rs`).
- Notable finding: The entire DOS-driver/stream/video-mode layer (Register*, SaveCtrlBreak, ScreenBuffer, ScreenMode, SetVideoMode, SetMemTop, SetBufferSize, smXXXX, ShowMouse, ShowMarkers, SpecialChars, StartupMode, stXXXX, StreamError, StoreHistory, StoreIndexes, SysColorAttr, SysErrActive, SysErrorFunc, SysMonoAttr, SystemError, RepeatDelay, PtrRec, PrintStr) is intentionally NOT-PORTED — 37 entries — representing the bulk of the DOS-era substrate that has no Rust analog. `ShowMarkers`/`SpecialChars` (monochrome focus indicators) are the only functional gaps that could be added without touching DOS infrastructure.
