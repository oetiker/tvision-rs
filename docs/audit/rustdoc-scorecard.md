# Rustdoc scorecard — public symbols below the "what + how" bar

Every public `PORTED`/`EQUIVALENT` symbol scoring < 3 on Axis 2 (docs). See [`README.md`](README.md) for the rubric. Back to [coverage-matrix](coverage-matrix.md).

**Total below bar:** 644 (1 undocumented · 36 signature-only · 607 what-but-not-how). The score-2 bulk is the dominant doc debt: the symbol says *what* it does but not *how/when to use it*.

## Priority queue — score 0 (undocumented) & 1 (signature-only)

| Section | Score | Symbol | What's missing |
|---|--:|---|---|
| Globals-317-330 | 1 | `tv::CD_NORMAL`, `tv::CD_NO_LOAD_DIR`, `tv::CD_HELP_BUTTON` (re-exp… | C++: `$0000/$0001/$0002`. Rust: `pub const CD_NORMAL: u16 = 0x0000` etc. in `src/dialog/filedlg.rs`, re-exported. Values match exactly. D… |
| Globals-317-330 | 0 | Private `cstrlen(s: &str) -> i32` (duplicated in `src/widgets/clust… | C++: `function CStrLen(S: String): Integer` — returns length of a tilde-hotkey string excluding the tilde characters. Rust: a private `fn… |
| Globals-331-346 | 1 | `MouseEventFlags::double_click: bool` on `MouseEvent` (`src/event/m… | C++ is a global `Word = 8` (units: 1/18.2 s) controlling double-click detection. Rust delegates double-click detection to crossterm/OS; t… |
| Globals-331-346 | 1 | `Editor::editor_flags: u16` (per-instance field; `src/widgets/edito… | C++ was a global `Word = efBackupFiles + efPromptOnReplace`. Rust makes it a per-instance field on `Editor` rather than a global; set/get… |
| Globals-331-346 | 1 | `EF_CASE_SENSITIVE`, `EF_WHOLE_WORDS_ONLY`, `EF_PROMPT_ON_REPLACE`,… | Six constants with matching values ($0001–$0100). All six are present. `pub(crate)` visibility — not public API. Score 1: named, no doc e… |
| Globals-331-346 | 1 | `Editor::find_str: String` (per-instance field; `src/widgets/editor… | C++ was a global `string[80]` holding the last search string across all editors. Rust makes it per-instance on `Editor`; coordinated acro… |
| Globals-331-346 | 1 | crossterm mouse cursor hide in `CrosstermBackend` (internal) | C++ increments "hide counter" in mouse driver. Rust hides/shows mouse cursor via crossterm on terminal setup/teardown; no public `hide_mo… |
| Globals-331-346 | 1 | `HISTORY_SIZE: usize = 1024` (`src/widgets/history.rs:72`, `pub(cra… | C++ `Word = 1024`: the byte budget for the history block. Rust is an internal constant with the same value. Not public; not documented be… |
| Globals-347-362 | 1 | `src/widgets/editor.rs`: `const MAX_LINE_LENGTH: i32 = 256` | Private constant used as the editor line-limit. Doc score 1: the constant is `pub(crate)`/private and carries only its value; publicly th… |
| Globals-363-378 | 1 | `Editor::replace_str` (per-instance field `String`) | C++: global `string[80]` shared across all editors. Rust: per-`Editor` instance field `replace_str: String`, accessed via `Editor::replac… |
| Globals-363-378 | 1 | `Backend::size() -> (u16, u16)` (second element) / `Program::deskto… | C++: global `Byte` set by `InitVideo`. Rust: queried on demand via `backend.size()` (no mutable global). Program stores live size in the … |
| Globals-363-378 | 1 | `Backend::size() -> (u16, u16)` (first element) / `Program::desktop… | C++: global `Byte` set by `InitVideo`. Same as `ScreenHeight` — dynamic query, no global. |
| Globals-363-378 | 1 | `Menu::builder()` with `Undo`/`Cut`/`Copy`/`Paste`/`Clear` items (u… | C++: returns a linked list of `TMenuItem` for the standard Edit menu. Rust: no single `std_edit_menu_items()` free function; the test `bu… |
| Globals-363-378 | 1 | `Menu::builder()` with New/Open/Save/SaveAs/SaveAll/ChangeDir/DosSh… | Same pattern as `StdEditMenuItems`. No free function — app builds via `MenuBuilder`. |
| Globals-363-378 | 1 | `Menu::builder()` with Tile/Cascade/CloseAll/Size-Move/Zoom/Next/Pr… | Same pattern as `StdFileMenuItems`/`StdEditMenuItems`. |
| Globals-582-586 | 1 | `is_word_char(ch: u8) -> bool` + `get_char_type(ch: u8) -> u8` in `… | Guide: set of characters treated as word characters by the editor (digits, letters, underscore). Rust: `is_word_char` and `get_char_type`… |
| TApplication | 1 | `Drop` for `Application` / `Program` (implicit) | Guide: calls `TProgram.Done` then shuts down all TV subsystems. Rust: `Backend` drop handles terminal teardown; history `thread_local` dr… |
| TCluster | 1 | `ClusterKind::icon()` + `Cluster::draw` (inlined) | Guide: `DrawBox(Icon: String; Marker: Char)` — called by subclass `Draw` to paint one column. Rust: inlined into `Cluster::draw` using `k… |
| TCluster | 1 | `Cluster::draw` (multi branch via `multi_mark`) | Guide: `DrawMultiBox(Icon, Marker: String)` for multi-state. Rust: same draw loop, branches on `ClusterKind::MultiCheckBoxes` using `mult… |
| TColorDialog | 1 | `InfoColumn` (`src/dialog/colorpick/info.rs`) — old/new color swatches | `InfoColumn` shows an "Old" and a "New" swatch continuously; equivalent color-preview role.  Doc score 1: module-level only. |
| TColorDisplay | 1 | `InfoColumn::handle_event` (`src/dialog/colorpick/info.rs:77`) | C++ responded to `cmNewColorIndex` to call `setColor`; Rust's `handle_event` is a no-op (passive view — the shared `ColorModel` Rc drives… |
| TColorDisplay | 1 | Implicit via `SharedModel` (`src/dialog/colorpick/model.rs`) — `Col… | C++ called `setColor(aColor)` to push a new color to the display.  Rust: `InfoColumn` reads `model.borrow().color` on every `draw` call; … |
| TColorItemList | 1 | `PresetsSurface::handle_event` arrow keys / mouse click (`src/dialo… | C++ `focusItem` broadcast `cmNewColorIndex` to update selectors.  Rust `PresetsSurface::handle_event` updates the `ColorModel` on each se… |
| TColorItemList | 1 | `PresetsSurface::draw` renders each row's name from `PRESETS[i].0` … | C++ fetched the item name from the linked list; Rust reads from the static `PRESETS` slice.  Equivalent display of named entries. |
| TColorItemList | 1 | `PresetsSurface::handle_event` (`src/dialog/colorpick/presets.rs:92`) | C++ handled keyboard/mouse events for the item list.  Rust handles Up/Down keys and mouse clicks in `PresetsSurface`.  Equivalent interac… |
| TCommandSet | 1 | `CommandSet::new()` (empty) + `+=` / `enable_cmd` calls | C++ allows Pascal set-literal initialization. Rust has no set-literal syntax; callers build incrementally. The `new()` doc only says "An … |
| TFileEditor | 1 | `tv::Editor::set_cmd_state(&self, command, enable, ctx)` | C++: `setCmdState(cmd, bool)` on `TEditor`. Rust: `Editor::set_cmd_state` — calls `ctx.enable_command` or `ctx.disable_command`. Same log… |
| TFileEditor | 1 | `tv::Editor::set_buf_size(&mut self, new_size) -> bool` | C++: `TFileEditor::setBufSize` grows via `malloc`/`memmove`; minimum 0x1000, alignment to 0x1000. Rust: `Editor::set_buf_size` grows the … |
| TFileEditor | 1 | `Editor::set_state(Active, false, ctx)` path in `View::set_state` | C++: `shutDown` calls `setCmdState(cmSave/cmSaveAs, False)` then `TEditor::shutDown`. Rust: `Editor::set_state` for `StateFlag::Active` w… |
| TInputLine | 1 | `InputLine::new(…, validator: Option<Box<dyn Validator>>, …)` const… | Guide: disposes existing validator, assigns new one. Rust: validator is set at construction time via `new`; there is no `set_validator` m… |
| TMenu | 1 | `Menu::default()` (`#[derive(Default)]`) | C++ `TMenu()` sets `items = deflt = 0`. Rust `Default` gives `items: vec![], default: None`. No doc on the derived impl beyond the struct… |
| TMenu | 1 | `Menu { items: ..., default: Some(n) }` struct literal (escape hatch) | C++ lets caller pass a separate default item. Rust: the struct fields are `pub`; the builder always uses `Some(0)`. A custom default requ… |
| TOutlineViewer | 1 | `tv::OV_EXPANDED: u16 = 0x01` | C++ `ovExpanded = 0x01`. Rust module-level const (not an assoc const — `OutlineViewer` is a trait, not a struct). Currently `pub(crate)`.… |
| TOutlineViewer | 1 | `tv::OV_CHILDREN: u16 = 0x02` | Same pattern. Pub(crate). |
| TOutlineViewer | 1 | `tv::OV_LAST: u16 = 0x04` | Same pattern. Pub(crate). |
| TOutlineViewer | 1 | `tv::command::Command::OUTLINE_ITEM_SELECTED` (= 301) | C++ `cmOutlineItemSelected = 301`. Rust: `Command::OUTLINE_ITEM_SELECTED`. Assoc const on the `Command` newtype. Doc: name only. |
| TStatusLine | 1 | `StatusLine.pressed_item: Option<usize>` field + `draw` | C++: `void drawSelect(TStatusItem *selected)` — the shared draw path for both `draw()` and the mouse-loop highlight. Rust: no separate `d… |

## Score-2 backlog by section (what present, how/when missing) — fix priority by count

| Section | s0 | s1 | s2 | total<3 |
|---|--:|--:|--:|--:|
| [Globals-363-378](reference/Globals-363-378.md) | 0 | 6 | 32 | 38 |
| [Globals-331-346](reference/Globals-331-346.md) | 0 | 6 | 27 | 33 |
| [TView](reference/TView.md) | 0 | 0 | 32 | 32 |
| [Globals-347-362](reference/Globals-347-362.md) | 0 | 1 | 30 | 31 |
| [TOutlineViewer](reference/TOutlineViewer.md) | 0 | 4 | 23 | 27 |
| [TFileDialog](reference/TFileDialog.md) | 0 | 0 | 21 | 21 |
| [TCluster](reference/TCluster.md) | 0 | 2 | 18 | 20 |
| [TEvent](reference/TEvent.md) | 0 | 0 | 20 | 20 |
| [Globals-317-330](reference/Globals-317-330.md) | 1 | 1 | 16 | 18 |
| [TScrollBar](reference/TScrollBar.md) | 0 | 0 | 16 | 16 |
| [TWindow](reference/TWindow.md) | 0 | 0 | 15 | 15 |
| [TInputLine](reference/TInputLine.md) | 0 | 1 | 13 | 14 |
| [TListViewer](reference/TListViewer.md) | 0 | 0 | 12 | 12 |
| [TMenuItem](reference/TMenuItem.md) | 0 | 0 | 12 | 12 |
| [TScroller](reference/TScroller.md) | 0 | 0 | 12 | 12 |
| [TFileEditor](reference/TFileEditor.md) | 0 | 3 | 8 | 11 |
| [TOutline](reference/TOutline.md) | 0 | 0 | 11 | 11 |
| [TRect](reference/TRect.md) | 0 | 0 | 11 | 11 |
| [TEditor](reference/TEditor.md) | 0 | 0 | 10 | 10 |
| [TMultiCheckBoxes](reference/TMultiCheckBoxes.md) | 0 | 0 | 10 | 10 |
| [TApplication](reference/TApplication.md) | 0 | 1 | 8 | 9 |
| [TGroup](reference/TGroup.md) | 0 | 0 | 9 | 9 |
| [TButton](reference/TButton.md) | 0 | 0 | 8 | 8 |
| [TMenuView](reference/TMenuView.md) | 0 | 0 | 8 | 8 |
| [TStatusLine](reference/TStatusLine.md) | 0 | 1 | 7 | 8 |
| [TCommandSet](reference/TCommandSet.md) | 0 | 1 | 6 | 7 |
| [THistory](reference/THistory.md) | 0 | 0 | 7 | 7 |
| [TIndicator](reference/TIndicator.md) | 0 | 0 | 7 | 7 |
| [TPoint](reference/TPoint.md) | 0 | 0 | 7 | 7 |
| [TTerminal](reference/TTerminal.md) | 0 | 0 | 7 | 7 |
| [Globals-582-586](reference/Globals-582-586.md) | 0 | 1 | 5 | 6 |
| [PrimitiveTypes](reference/PrimitiveTypes.md) | 0 | 0 | 6 | 6 |
| [TChDirDialog](reference/TChDirDialog.md) | 0 | 0 | 6 | 6 |
| [TDialog](reference/TDialog.md) | 0 | 0 | 6 | 6 |
| [TEditWindow](reference/TEditWindow.md) | 0 | 0 | 6 | 6 |
| [TFileInfoPane](reference/TFileInfoPane.md) | 0 | 0 | 6 | 6 |
| [TFileInputLine](reference/TFileInputLine.md) | 0 | 0 | 6 | 6 |
| [TFileList](reference/TFileList.md) | 0 | 0 | 6 | 6 |
| [THistoryViewer](reference/THistoryViewer.md) | 0 | 0 | 6 | 6 |
| [THistoryWindow](reference/THistoryWindow.md) | 0 | 0 | 6 | 6 |
| [TMenu](reference/TMenu.md) | 0 | 2 | 4 | 6 |
| [TNode](reference/TNode.md) | 0 | 0 | 6 | 6 |
| [TProgram](reference/TProgram.md) | 0 | 0 | 6 | 6 |
| [TRadioButtons](reference/TRadioButtons.md) | 0 | 0 | 6 | 6 |
| [TCheckBoxes](reference/TCheckBoxes.md) | 0 | 0 | 5 | 5 |
| [TDirEntry](reference/TDirEntry.md) | 0 | 0 | 5 | 5 |
| [TDirListBox](reference/TDirListBox.md) | 0 | 0 | 5 | 5 |
| [TListBox](reference/TListBox.md) | 0 | 0 | 5 | 5 |
| [TSortedListBox](reference/TSortedListBox.md) | 0 | 0 | 5 | 5 |
| [TColorDisplay](reference/TColorDisplay.md) | 0 | 2 | 2 | 4 |
| [TFileCollection](reference/TFileCollection.md) | 0 | 0 | 4 | 4 |
| [TFilterValidator](reference/TFilterValidator.md) | 0 | 0 | 4 | 4 |
| [TFrame](reference/TFrame.md) | 0 | 0 | 4 | 4 |
| [TLabel](reference/TLabel.md) | 0 | 0 | 4 | 4 |
| [TMemo](reference/TMemo.md) | 0 | 0 | 4 | 4 |
| [TStatusDef](reference/TStatusDef.md) | 0 | 0 | 4 | 4 |
| [TBackground](reference/TBackground.md) | 0 | 0 | 3 | 3 |
| [TColorDialog](reference/TColorDialog.md) | 0 | 1 | 2 | 3 |
| [TColorItemList](reference/TColorItemList.md) | 0 | 3 | 0 | 3 |
| [TDesktop](reference/TDesktop.md) | 0 | 0 | 3 | 3 |
| [TDrawBuffer](reference/TDrawBuffer.md) | 0 | 0 | 3 | 3 |
| [TMenuBox](reference/TMenuBox.md) | 0 | 0 | 3 | 3 |
| [TPXPictureValidator](reference/TPXPictureValidator.md) | 0 | 0 | 3 | 3 |
| [TParamText](reference/TParamText.md) | 0 | 0 | 3 | 3 |
| [TRangeValidator](reference/TRangeValidator.md) | 0 | 0 | 3 | 3 |
| [TSearchRec](reference/TSearchRec.md) | 0 | 0 | 3 | 3 |
| [TStatusItem](reference/TStatusItem.md) | 0 | 0 | 3 | 3 |
| [TStringLookupValidator](reference/TStringLookupValidator.md) | 0 | 0 | 3 | 3 |
| [TVTransfer](reference/TVTransfer.md) | 0 | 0 | 3 | 3 |
| [TValidator](reference/TValidator.md) | 0 | 0 | 3 | 3 |
| [TMenuBar](reference/TMenuBar.md) | 0 | 0 | 2 | 2 |
| [TStaticText](reference/TStaticText.md) | 0 | 0 | 2 | 2 |
| [TTextDevice](reference/TTextDevice.md) | 0 | 0 | 2 | 2 |
| [TDirCollection](reference/TDirCollection.md) | 0 | 0 | 1 | 1 |
| [TLookupValidator](reference/TLookupValidator.md) | 0 | 0 | 1 | 1 |
| [TMemoData](reference/TMemoData.md) | 0 | 0 | 1 | 1 |
| [TMonoSelector](reference/TMonoSelector.md) | 0 | 0 | 1 | 1 |
| [TPalette](reference/TPalette.md) | 0 | 0 | 1 | 1 |
| [TScrollChars](reference/TScrollChars.md) | 0 | 0 | 1 | 1 |
| [TStrListMaker](reference/TStrListMaker.md) | 0 | 0 | 1 | 1 |
| [TStringList](reference/TStringList.md) | 0 | 0 | 1 | 1 |