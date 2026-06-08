# Turbo Vision → Rust: Dependency-Ordered Porting Checklist

Mechanical execution order for the faithful port. A class is listed only after
its base class **and** every class it owns/constructs has been listed. Read
[docs/PORTING-GUIDE.md](file:///home/oetiker/checkouts/rstv/docs/PORTING-GUIDE.md)
first — the `D1`–`D13` references below point at it.

**Source-file convention (magiblot C++ tree at
`/home/oetiker/scratch/tvision-spec/magiblot-tvision/source/tvision/`):** **port
whatever file the `C++ files` column cites, regardless of prefix.** Typically a
class has a `tXXX.cpp` (member functions) plus `sXXX.cpp` / `nmXXX.cpp`
(TStreamable registration — usually droppable per D12). **But the `s*` file is
not always streamable-only:** several classes keep their member implementations
there — the color selectors live in `colorsel.cpp` + `sclrsel.cpp`, and the
`stddlg` leaf views (`TFileInputLine`, `TFileInfoPane`, `TSortedListBox`) have
their member code in `stddlg.cpp`/`sfinputl.cpp`/`sfinfpne.cpp` with no `t*`
counterpart. Do **not** blanket-delete `s*`/`nm*`; only drop the streamable
`build`/`read`/`write` boilerplate (D12). Pure tooling/util TUs (`geninc`,
`prntcnst`, `new`, `newstr`, `snprintf`, `fmtstr`, `misc`, all `.asm`) are not
class implementations and are excluded.

**Irregular mappings worth flagging up front** (verified against the clone):
- `TPoint`/`TRect`/`TScreenCell`/`TColorAttr`/`TColorDesired` have **no `.cpp`** —
  they are header-inline in `objects.h` / `scrncell.h` / `colors.h`.
- `TDrawBuffer` methods live in `drivers.cpp` (not a `drawbuf.cpp`).
- `TText` methods live in `drivers.cpp`/`drivers2.cpp` (not a `ttext.cpp`);
  `tvtext1.cpp`/`tvtext2.cpp` are **static glyph/string tables** that become the
  Theme `Glyphs` set (D7), not a class.
- Color quantization ladder is **`source/platform/colors.cpp`** (the
  `RGBtoHCL`/`RGBtoXTerm16` algorithm + LUTs) plus the inline conversion
  functions in **`include/tvision/colors.h`** (`RGBtoXTerm256`, `BIOStoXTerm16`,
  `XTerm256toRGB`, …). **Not** `mapcolor.cpp`/`palette.cpp` — those are the D7
  palette-chain walk (`TView::mapColor`) and the `TPalette` container, which
  belong to row 16 (`Theme`), not the row-5 quantization ladder.

**Tags:** `FOUNDATION` (pattern-setting; many deviations collide; careful
first-time work), `MECHANICAL` (leaf/transcription once foundation exists),
`INFRA` (net-new, no C++ source — built per the deviations).

**Status:** **✅ in the `#` column = ported & on `main`** (per-row detail in
[`docs/IMPLEMENTATION-LOG.md`](file:///home/oetiker/checkouts/rstv/docs/IMPLEMENTATION-LOG.md)).
Unmarked rows are the remaining work — the **lowest-numbered unmarked row is
next**. As of this writing rows **1–64 are done** (63 = msgbox/inputBox, 64 =
`StringList`); **66 (`TEditor`) core is ◑** (65 is not a porting row; 66
sub-features deferred — see row); **67 (`TMemo`) is next**.
(Rows 77+ are not in this excerpt's range — keep marking as they land.) **Beyond the ladder:** `RegexValidator` (in `validate`) is an
**rstv-original extension** — a regex-driven validator alongside the faithful
`TPXPictureValidator` (62) — not a Turbo Vision class, so it has no row here.

**Out of scope** (present in `tv.h` but in no guide module): the **help system**
(`helpbase.cpp`/`help.cpp` → `THelpViewer`/`THelpWindow`/`THelpFile`) and
**surface** (`TSurfaceView`/`TDrawSurface`, `tsurface.cpp`). Excluded here;
revisit only if a guide module is added. **Persistence** (`TStreamable`,
`TResourceFile`, `ipstream`/`opstream`) is dropped wholesale per D12.

---

## Phase 0 — Primitives & infrastructure

Data types and the net-new rendering/runtime substrate. The `INFRA` rows have no
C++ source — they are the deviation machinery (D3/D8/D9/D11). `mapcolor.cpp`'s
quantization ladder is a **faithful port** that *lives inside* the Backend, so
the Backend row is "net-new trait wrapping ported code," not write-from-scratch.

| # | Class | Base | C++ files | Rust module | Tag | Notes / owns |
|---|-------|------|-----------|-------------|-----|--------------|
| 1 ✅ | `TPoint` | — | `objects.h` (inline) | `view` (geometry) | FOUNDATION | x/y; arithmetic ops |
| 2 ✅ | `TRect` | — | `objects.h` (inline) | `view` (geometry) | MECHANICAL | a/b corners; intersect/union/move/grow; owns 2×`TPoint` |
| 3 ✅ | `TColorRGB`/`TColorDesired` → `Color` | — | `colors.h` (inline) | `color` | FOUNDATION | D6 four-variant enum (Default/Bios/Indexed/Rgb) |
| 4 ✅ | `TColorAttr` → `Style` | — | `colors.h` (inline) | `color` | FOUNDATION | D6 fg/bg + `Modifiers` (reverse, no-shadow); owns `Color` |
| 5 ✅ | quantization ladder | — | `platform/colors.cpp`, `colors.h` (inline) | `backend` (`quantize`) | INFRA* | D6 RGB→256→16→BIOS faithful port; lives in Backend. (Not `mapcolor.cpp`/`palette.cpp` — those are D7/row 16.) |
| 6 ✅ | `TScreenCell` → `Cell` | — | `scrncell.h` (inline) | `screen` | FOUNDATION | char(s)+`Style`; vendored ratatui cell shape |
| 7 ✅ | `TDrawBuffer` | — | `drivers.cpp` | `screen` (`DrawBuffer`) | FOUNDATION | moveStr/moveChar/moveBuf/putAttribute; owns `Cell`s |
| 8 ✅ | `TText` | — | `drivers.cpp`, `drivers2.cpp` | `text` | FOUNDATION | D13 width/scroll/cell-writer; `unicode-width`+`-segmentation` |
| 9 ✅ | glyph/string tables | — | `tvtext1.cpp`, `tvtext2.cpp` | `theme` (`Glyphs`) | MECHANICAL | D7 frame/scrollbar/marks/icons → `Glyphs` |
| 10 ✅ | `TKey` + key events | — | `tkey.cpp`, `tkeys.h` | `event` (`Key`) | FOUNDATION | D5 decomposed `enum Key` + `KeyModifiers` bool struct + `KeyEvent`; no modifier-combined variants (Ctrl+C = `Key::Char('c')` + ctrl, Shift+Tab = `Key::Tab` + shift). Mirrors magiblot canonical `TKey` (base key + modifier flags + `controlKeyState`). Not BIOS scancodes |
| 11 ✅ | `TEvent`/`MouseEventType`/`KeyDownEvent`/`MessageEvent` | — | `tevent.cpp`, `system.h` | `event` | FOUNDATION | D4 `enum Event` sum type; `EventMask` bool struct |
| 12 ✅ | `TCommandSet` | — | `tcmdset.cpp` | `command` | FOUNDATION | D1 → `CommandSet` over `HashSet<Command>`; `Command(&'static str)` open newtype (namespaced); no range guard, no `all()`; external views mint via `Command::custom("ns.name")`. Enabled-by-default policy moves to `TView`/`TProgram` (rows 23/31). View-specific commands live with their view module, not centralized. |
| 13 ✅ | `TObject` | — | `tobject.cpp` | (absorbed) | FOUNDATION | D2 no root class; lifetime via Rust ownership/`Drop` |
| 14 ✅ | `TNSCollection`/`TCollection` | `TObject` | `tcollect.cpp`, `tvobjs.h` | (idiom) | MECHANICAL | → `Vec<T>` + iterators; `firstThat`/`forEach` → iterators |
| 15 ✅ | `TNSSortedCollection`/`TSortedCollection` | `TCollection` | `tsortcol.cpp` | (idiom) | MECHANICAL | → `Vec<T: Ord>` |
| 15a ✅ | `TStringCollection` | `TSortedCollection` | `tstrcoll.cpp`, `sstrcoll.cpp` | (idiom) | MECHANICAL | → sorted `Vec<String>`; needed by `TStringLookupValidator` (#61) |
| 16 ✅ | `Theme` | — | (synthesizes D7 palettes) | `theme` | INFRA | Role→Style map + `Glyphs`; default = classic blue (`cpAppColor`) |
| 17 ✅ | `ViewId` minter | — | (replaces `owner`/`current`/`next`) | `view` (`ViewId`) | INFRA | D3 global monotonic `ViewId` identity; up/sideways links by id |
| 18 ✅ | renderer back-buffer + diff | — | (replaces `TVWrite`/`drawUnder*`) | `screen` | INFRA | D8 whole-tree redraw + cell diff; vendored ratatui `Buffer` |
| 19 ✅ | `Backend` trait + `CrosstermBackend` + `HeadlessBackend` | — | `THardwareInfo`/`TScreen`/`TClipboard` (`tscreen.cpp`, `hardwrvr.cpp`, `tclipbrd.cpp`) as design ref | `backend` | INFRA | D11; size/flush/cursor/clipboard; wraps row 5 ladder |
| 20 ✅ | `Clock` + timer queue | — | `TTimerQueue` (`ttimerqu.cpp`) as ref | `timer` | INFRA | D9/D11 injected clock, cancelable handles, poll timeout |
| 21 ✅ | capture stack | — | (replaces nested `execView`/`dragView` loops) | `capture` | INFRA | D9 LIFO handlers; modal/drag/press = handlers |
| 22 ✅ | `Context` / `DrawCtx` | — | (replaces up-pointers + clip) | `view` | INFRA | D3 downward ctx: theme/clip/parent style; targeted query (D4) |

---

## Phase 1 — Foundation views & program shell

| # | Class | Base | C++ files | Rust module | Tag | Notes / owns |
|---|-------|------|-----------|-------------|-----|--------------|
| 23 ✅ | `TView` | `TObject` | `tview.cpp`, `sview.cpp`, `tvexposd.cpp`, `tvcursor.cpp` | `view` | FOUNDATION | D2 `View` trait + `ViewState`; D5 state/options/growMode/dragMode structs; pattern-setting class |
| 24 ✅ | `TFrame` | `TView` | `tframe.cpp`, `sframe.cpp`, `framelin.cpp` | `frame` | FOUNDATION | window border/title/icons; glyphs from Theme (D7) |
| 25 ✅ | `TScrollBar` | `TView` | `tscrlbar.cpp`, `sscrlbar.cpp` | `widgets::scrollbar` | MECHANICAL | value/min/max/step; `cmScrollBarChanged` broadcast |
| 26 ✅ | `TGroup` | `TView` | `tgroup.cpp`, `grp.cpp`, `sgroup.cpp`, `tgrmv.cpp` | `group` | FOUNDATION | D3 owns `Vec<Box<dyn View>>`; D4 three-phase routing; D8 drop buffered/lock; `current` via `ViewId` |
| 27 ✅ | `TScroller` | `TView` | `tscrolle.cpp`, `sscrolle.cpp` | `widgets::scroller` | MECHANICAL | takes 2×`TScrollBar` (→25); `delta`/`limit` |
| 28 ✅ | `TListViewer` | `TView` | `tlstview.cpp`, `slstview.cpp` | `widgets::listviewer` | FOUNDATION | takes 2×`TScrollBar` (→25); list-render matrix roles (D7); base for list widgets |
| 29 ✅ | `TBackground` | `TView` | `tbkgrnd.cpp`, `sbkgrnd.cpp`, `nmbkgrnd.cpp` | `desktop` | MECHANICAL | pattern fill |
| 30 ✅ | `TDeskTop` | `TGroup` + `TDeskInit` | `tdesktop.cpp`, `sdesktop.cpp`, `nmdsktop.cpp` | `desktop` | FOUNDATION | owns `TBackground` (→29) via factory mixin; tile/cascade |
| 31 ✅ | `TProgram` | `TGroup` + `TProgInit` | `tprogram.cpp` | `app` | FOUNDATION | **factory-mixin deferral:** holds `TStatusLine`/`TMenuBar`/`TDeskTop` via injected factories — those classes are Phase 4 yet `TProgram` precedes them. Owns the single event loop (D9), timer queue (→20). |
| 32 ✅ | `TApplication` | `TProgram` + `TAppInit` | `tapplica.cpp` | `app` | MECHANICAL | tile/cascade/dosShell wrappers over `TProgram` |

---

## Phase 2 — Windows & dialogs

| # | Class | Base | C++ files | Rust module | Tag | Notes / owns |
|---|-------|------|-----------|-------------|-----|--------------|
| 33 ✅ | `TWindow` | `TGroup` + `TWindowInit` | `twindow.cpp`, `swindow.cpp`, `nmwindow.cpp` | `window` | FOUNDATION | builds `TFrame` (→24) via factory mixin; `standardScrollBar` (→25); zoom/move/close; D2 embed-and-delegate exemplar |
| 34 ✅ | `TDialog` | `TWindow` | `tdialog.cpp`, `sdialog.cpp`, `nmdialog.cpp` | `dialog` | FOUNDATION | modal via capture handler (D9); `cmOK`/`cmCancel`; gather/scatter typed values (D10) |

---

## Phase 3 — Simple widgets (mostly independent leaves)

`TInputLine` needs the **abstract `Validator` trait** (row 35) but not the
concrete validators (Phase 5). Split accordingly.

| # | Class | Base | C++ files | Rust module | Tag | Notes / owns |
|---|-------|------|-----------|-------------|-----|--------------|
| 35 ✅ | `TValidator` (abstract) | `TObject` | `tvalidat.cpp`, `svalid.cpp` | `validate` | FOUNDATION | D2 `Validator` trait: `is_valid_input`/`is_valid`/`transfer` (D10) |
| 36 ✅ | `TStaticText` | `TView` | `tstatict.cpp`, `sstatict.cpp` | `widgets::static_text` | MECHANICAL | word-wrap text draw (D13) |
| 37 ✅ | `TButton` | `TView` | `tbutton.cpp`, `sbutton.cpp` | `widgets::button` | MECHANICAL | press animation via Clock (→20); shadow glyphs (D7); broadcast/command flags |
| 38 ✅ | `TCluster` | `TView` | `tcluster.cpp`, `scluster.cpp` | `widgets::cluster` | FOUNDATION | owns label strings; base for check/radio; `value`/`enableMask` bits |
| 39 ✅ | `TInputLine` | `TView` | `tinputli.cpp`, `sinputli.cpp` | `widgets::input_line` | FOUNDATION | holds optional `Validator` (→35); typed `value`/`set_value` (D10); selection; arrows glyphs (D7) |
| 40 ✅ | `TParamText` | `TStaticText` | `tparamte.cpp`, `sparamte.cpp` | `widgets::static_text` | MECHANICAL | printf-style formatted static text |
| 41 ✅ | `TLabel` | `TStaticText` | `tlabel.cpp`, `slabel.cpp` | `widgets::label` | MECHANICAL | `link` to a control via `ViewId` (D3); focus-on-shortcut |
| 42 ✅ | `TCheckBoxes` | `TCluster` | `tcheckbo.cpp`, `scheckbo.cpp`, `nmchkbox.cpp` | `widgets::cluster` | MECHANICAL | check marks (D7) |
| 43 ✅ | `TRadioButtons` | `TCluster` | `tradiobu.cpp`, `sradiobu.cpp`, `nmrbtns.cpp` | `widgets::cluster` | MECHANICAL | radio marks (D7) |
| 44 ✅ | `TMultiCheckBoxes` | `TCluster` | `tmulchkb.cpp`, `smulchkb.cpp`, `nmmulchk.cpp` | `widgets::cluster` | MECHANICAL | multi-state marks; `states` array |
| 45 ✅ | `TIndicator` | `TView` | `tindictr.cpp`, `editstat.cpp` | `widgets::indicator` | MECHANICAL | editor row/col + modified flag display |

---

## Phase 4 — Lists, menus, status line, history

| # | Class | Base | C++ files | Rust module | Tag | Notes / owns |
|---|-------|------|-----------|-------------|-----|--------------|
| 46 ✅ | `TMenuItem`/`TSubMenu`/`TMenu` | — | `menu.cpp` | `menu` | FOUNDATION | menu data tree (name/command/key/submenu); `operator+` builders → Rust builder API |
| 47 ✅ | `TStatusItem`/`TStatusDef` | — | `menus.h` (inline) | `status` | MECHANICAL | status-item data (text/key/cmd, help-ctx ranges) |
| 48 ✅ | `TListBox` | `TListViewer` | `tlistbox.cpp`, `slistbox.cpp`, `nmlstbox.cpp` | `widgets::list_box` | MECHANICAL | owns a `TCollection` (→14); takes `TScrollBar` (→25); typed value (D10) |
| 49 ✅ | `TMenuView` | `TView` | `tmnuview.cpp`, `smnuview.cpp` | `menu` | FOUNDATION | holds `TMenu` (→46); hotkey/shortcut dispatch; `evBroadcast` mask |
| 50 ✅ | `TMenuBar` | `TMenuView` | `tmenubar.cpp`, `smenubar.cpp` | `menu` | MECHANICAL | horizontal bar layout |
| 51 ✅ | `TMenuBox` | `TMenuView` | `tmenubox.cpp`, `smenubox.cpp` | `menu` | MECHANICAL | vertical popup box; frame glyphs (D7) |
| 52 ✅ | `TMenuPopup` | `TMenuBox` | `tmenupop.cpp`, `smenupop.cpp`, `popupmnu.cpp` | `menu` | MECHANICAL | spawns/execs popup (D9); `popupMenu()` free fn in `popupmnu.cpp` |
| 53 ✅ | `TStatusLine` | `TView` | `tstatusl.cpp`, `sstatusl.cpp` | `status` | FOUNDATION | owns `TStatusDef`/`TStatusItem` (→47); hint(); help-ctx → hint mapping |
| 54 ✅ | history store (`historyAdd`/`Count`/`Str`/`clearHistory`) | — | `histlist.cpp` | `widgets::history` | MECHANICAL | per-id ring buffer backing store |
| 55 ✅ | `THistoryViewer` | `TListViewer` | `thstview.cpp` | `widgets::history` | MECHANICAL | reads history store (→54) |
| 56 ✅ | `THistoryWindow` | `TWindow` + `THistInit` | `thistwin.cpp` | `widgets::history` | MECHANICAL | owns `THistoryViewer` (→55) via factory mixin |
| 57 ✅ | `THistory` | `TView` | `thistory.cpp`, `nmhist.cpp` | `widgets::history` | MECHANICAL | dropdown icon next to `TInputLine` (→39); spawns `THistoryWindow` (→56) |

---

## Phase 5 — Advanced (validators, editors, std dialogs, file/color/outline/text)

| # | Class | Base | C++ files | Rust module | Tag | Notes / owns |
|---|-------|------|-----------|-------------|-----|--------------|
| 58 ✅ | `TFilterValidator` | `TValidator` | `tvalidat.cpp` | `validate` | MECHANICAL | char allow-list |
| 59 ✅ | `TRangeValidator` | `TFilterValidator` | `tvalidat.cpp` | `validate` | MECHANICAL | numeric min/max; `transfer` (D10) |
| 60 ✅ | `TLookupValidator` | `TValidator` | `tvalidat.cpp` | `validate` | MECHANICAL | abstract lookup |
| 61 ✅ | `TStringLookupValidator` | `TLookupValidator` | `tvalidat.cpp` | `validate` | MECHANICAL | owns string list |
| 62 ✅ | `TPXPictureValidator` | `TValidator` | `tvalidat.cpp` | `validate` | MECHANICAL | Paradox picture-mask state machine |
| 63 ✅ | `messageBox`/`messageBoxRect`/`inputBox`/`inputBoxRect` | — | `msgbox.cpp` | `dialog` (msgbox) | MECHANICAL | **all four ✅.** `Program::message_box`/`message_box_rect` (PART 1); `Program::input_box`/`input_box_rect` (PART 2) via the **single-input scatter/gather seam** (`exec_view_with_completion` gained a `gather: ViewId` + `(Command, Option<FieldValue>)` return; scatter = `set_value` pre-exec, gather = `value()` pre-drop gated on `!= CANCEL`). The general D10 dialog group-walk (`Dialog::value`/`set_value`) stays DEFERRED to its first multi-field consumer (Batch E). |
| 64 ✅ | `TStringList`/`TStrListMaker`/`TStrIndexRec` | `TObject` | `tstrlist.cpp`, `sstrlst.cpp` | `text` (resource) | MECHANICAL | **✅ minimal port.** Pure D12 case (all three classes are streaming-only, zero in-framework consumers) → ported only the observable contract: `StringList` in `src/text.rs`, a `BTreeMap<u16,String>` wrapper (`insert`/`get→Option`/`len`/`FromIterator`). Storage format + `TStreamable` machinery dropped. Renders nothing → unit tests only. |
| 66 ◑ | `TEditor` | `TView` | `teditor1.cpp`, `teditor2.cpp`, `edits.cpp` | `widgets::editor` | FOUNDATION | **◑ core complete.** Gap buffer + nav + edit + undo + selection + draw + keyboard handleEvent + search + D3 brokers (new `SyncEditorDelta`/`IndicatorSetValue` + clipboard `SetClipboard`/`EditorPaste`; `Role::ScrollerSelected` added) + system-clipboard cut/copy/paste. **Deferred (breadcrumbed):** find/replace dialogs (`search()` itself is live), mouse drag-select/edge-scroll/wheel, right-click context menu, internal-clipboard-editor branch (→69), TStreamable. |
| 67 ✅ | `TMemo` | `TEditor` | `tmemo.cpp` | `widgets::editor` | MECHANICAL | **✅** D2 embed-delegate wrapper over `Editor` (`#[delegate(to = editor)]`, no skip — `as_any_mut` delegates so the editor brokers reach through). Overrides: `handle_event` (swallow plain Tab → dialog focus-nav), `value`/`set_value` (D10 typed `FieldValue::Text`, via new inherent `Editor::set_text` = C++ `setData`). Dropped: `dataSize` (D10), `getPalette`/`cpMemo` (D7 — reuses editor draw; distinct memo roles deferred). Also fixed a latent row-66 `Editor` bug: Shift+Tab (`kbShiftTab`, charCode 0) was wrongly insertable. |
| 68 ✅ | `TFileEditor` | `TEditor` | `tfiledtr.cpp` | `widgets::editor` | MECHANICAL (FOUNDATION buffer change) | **✅ core.** D2 embed-delegate `FileEditor { editor, file_name }`. **FOUNDATION:** the inner `Editor` gained a flag-gated **growable buffer** (`file_editor` flag; `set_buf_size(&mut self)` grow branch: round to 0x1000, `Vec::resize` + `copy_within` tail-move; `new_file_editor` ctor; `update_commands` save-enable) — base/`Memo` fixed-buffer behavior provably unchanged (regression tests). `load_file`/`save_file`/`save` over real `std::fs`; `handle_event` cmSave (+flush); `valid` cmValid case. **Deferred (forced — breadcrumbed):** saveAs/SAVE_AS/untitled-save (needs `TFileDialog`), all `editorDialog` error/confirm popups + `valid()` modified-prompt (need async modal-from-view), `efBackupFiles`, `shutDown`, DOS 16-bit/OOM guards (Vec infallible), setBufSize shrink, TStreamable. |
| 69 ✅ | `TEditWindow` | `TWindow` | `teditwnd.cpp` | `widgets::editor` | MECHANICAL (integration) | **✅** D2 embed-delegate `EditWindow` over `Window`. Ctor wires hidden `ScrollBar`×2 + `Indicator` (inserted first → ids) into a `FileEditor` over the inner extent (ViewId-at-insertion order); `ofTileable`; title = filename/"Untitled"; `size_limits` min {24,6} (+`calc_bounds` skipped so the minimum survives owner resize). Hidden aux children are excluded from `reset_current` so the editor becomes current (→ shows the bars on active). **Deferred (breadcrumbed):** dynamic `getTitle`/`cmUpdateTitle` refresh (lands with `saveAs`/`TFileDialog`), `close()` clipboard branch (no rstv `close()` View method; clipboard editor unported), `TStreamable`. |
| 70 ✅ | `TSortedListBox` | `TListBox` | `stddlg.cpp` (member code), `sfilelst.cpp` | `widgets::list_box` | MECHANICAL | **✅** `SortedListBox` = D2 embed-delegate over `ListBox`; type-to-search incremental search (`handle_event` re-seeds `curString` from the **focused item** each keystroke, `search_pos` indexes into it; Backspace/'.'/char branches; binary search for the first item ≥ key). **No `TSortedCollection`** — folds a case-insensitively-sorted `Vec<String>` + binary search (rstv replaced `TCollection` with `Vec<String>`; rows 72/74 hold their own typed sorted Vecs). Sort + search + prefix-confirm all case-insensitive (coherence; deliberate rstv choice). `get_key` identity, `shift_state` stored-unused — breadcrumbed. |
| 71 ✅ | `TDirEntry` | — | `stddlg.h` (inline) | `dialog` (filedlg) | MECHANICAL | **✅** `DirEntry { display_text, directory }` + accessors. In `src/dialog/filedlg.rs` (batched 71–74). |
| 72 ✅ | `TDirCollection` | `TCollection` | `tdircoll.cpp`, `sdircoll.cpp`, `nmdircol.cpp` | `dialog` (filedlg) | MECHANICAL | **✅** `type DirCollection = Vec<DirEntry>` (collections→Vec; C++ `TCollection` API dropped — no consumer). |
| 73 ✅ | `TSearchRec` | — | `stddlg.h` (inline) | `dialog` (filedlg) | MECHANICAL | **✅** `SearchRec { attr:u8, time:i32, size:i32, name }`. attr/time/size populated by the deferred fs-read layer (breadcrumb). |
| 74 ✅ | `TFileCollection` | `TSortedCollection` | `tfilecol.cpp`, `sfilcoll.cpp`, `nmfilcol.cpp` | `dialog` (filedlg) | MECHANICAL | **✅** `FileCollection` = `Vec<SearchRec>` + verbatim `search_rec_compare` (".." last, dirs after files, case-sensitive name) + sorted `insert`; unused `TSortedCollection` API dropped. |
| 75 ✅ | `TDirListBox` | `TListBox` | `tdirlist.cpp`, `sdirlist.cpp`, `nmdirbox.cpp` | `dialog` (filedlg) | MECHANICAL (D14 design) | **✅** `DirListBox` = a **direct `ListViewer` impl** over `Vec<DirEntry>` (NOT a D2 delegate — a delegate would consult `ListBox`'s `Vec<String>` `get_text`). New **deviation D14 (native Linux `/` paths)**: `showDrives`/drive-letters/"Drives" entry/`\` all dropped; `showDirs` → pure `build_tree` (root `/` + `/`-segment ancestors + `read_dir` subdirs, sorted ci, dotfiles skipped, symlinks followed like magiblot's `stat`) split from the FS read for snapshot-testability; faithful last-entry glyph fix-up (`└─┬`→`└──` leaf corner, runs unconditionally on the last entry). **Breadcrumbed → row 80 `TChDirDialog`:** `select_item`'s `cmChangeDir`+`DirEntry` payload (rstv `Event::Broadcast` is payload-less) and `set_state`'s `chDirButton->makeDefault` owner poke. D12 (`TStreamable`) dropped. |
| 76 ✅ | `TFileList` | `TSortedListBox` | `tfillist.cpp`, `sfilelst.cpp`, `nmfillst.cpp` | `dialog` (filedlg) | MECHANICAL (FOUNDATION seam) | **✅** `FileList` = a **direct `ListViewer` + `SortedSearch` impl** over `Vec<SearchRec>` (NOT a D2 delegate over `SortedListBox` — same `get_text`-over-own-collection reason as row 75). **FOUNDATION precursor (separate commit d79813e):** extracted `TSortedListBox`'s type-to-search machine into `sorted_handle_event`/`sorted_cursor` free fns over a new `SortedSearch: ListViewer` sub-trait (`list_viewer.rs`); converted `SortedListBox` to a direct impl too. `FileList` overrides `search` to fuse C++ `getKey` (key `SearchRec` with `attr=FA_DIREC` from shift/dot, **no `strupr`** — `__FLAT__`/Linux) + `list()->search` via `search_rec_compare` over raw recs (the attr routes into the dir section). `num_cols=2`; `get_text` = name + `/` for dirs (D14); `read_directory` via `std::fs` (pure `build_listing` split from the fs read like row 75; files filtered by a `*`/`?` `wildcard_match`, dirs always shown, `..` unless root, follow symlinks). `value()=None` (getData/dataSize 0). **Breadcrumbed → row 79 `TFileDialog`:** all three owner broadcasts (`focusItem`/`readDirectory` → `cmFileFocused`, `selectItem` → `cmFileDoubleClicked`) are payload-carrying; **→ row 78:** `time`/date packing. D12 streaming + `fexpand`/`squeeze` DOS path machinery dropped. |
| 77 ✅ | `TFileInputLine` | `TInputLine` | `sfinputl.cpp` + `stddlg.cpp` (member code; no `t*` file) | `dialog` (filedlg) | MECHANICAL (FOUNDATION seam) | **✅** `FileInputLine`, a D2 embed-delegate over `InputLine`. **FOUNDATION precursor:** the **payload-carrying-broadcast seam** — rstv's `Event::Broadcast` is payload-less (D4), so `cmFileFocused`-carrying-a-`TSearchRec` is ported as a payload-less broadcast whose `source` is the resolvable subject, resolved by the pump's new `Deferred::ResolveFocusedFile` broker (the `cmScrollBarChanged` shape). A defaulted `ListViewer::on_focus_changed` hook (called at the tail of the `focus_item` funnel — the faithful translation of the virtual `TListViewer::focusItem`) makes `FileList` broadcast `FILE_FOCUSED {source=self}` on **every** focus change. `FileInputLine` filters the broadcast (while `!sfSelected`), requests the broker, and `on_file_focused` copies the focused name, appending `/<wildCard>` for dirs (D14 `/`). `as_any_mut`→`self` (opposite of `Memo`) so the broker downcasts to it. `cmFileDoubleClicked` is faithfully payload-less (the only consumer turns it into cmOK). New `Command` family (`FILE_OPEN/REPLACE/CLEAR/INIT`, `CHANGE_DIR`, `REVERT`, `FILE_FOCUSED`, `FILE_DOUBLE_CLICKED`). D12 dropped. |
| 78 ✅ | `TFileInfoPane` | `TView` | `sfinfpne.cpp` + `stddlg.cpp` (member code; no `t*` file) | `dialog` (filedlg) | MECHANICAL | **✅** `FileInfoPane`, a plain `TView` (second consumer of the row-77 `ResolveFocusedFile` broker — pump arm extended with an `else if … FileInfoPane` downcast). Caches `directory`/`wild_card`/`file_block` (draw has no `Context`/owner — D3); draws the path line + name/size/date line at the faithful `size.x − N` columns (no `!sfSelected` guard, unlike the input line). **D-time deviation:** `build_listing` now packs `std::fs` mtime into `SearchRec.time` as a DOS `ftime` u32 (so the bitfield unpack ports verbatim), computed in **UTC** (no tz crate) via Hinnant's days-from-civil; pre-1980 clamps to the DOS epoch; far-future (≥2044) intentionally sets the `i32` sign bit and round-trips via `as u32`; synthesized `..` uses `DOTDOT_TIME` unconditionally (cosmetic vs C++ which stats the real parent). `Role::InfoPane` (cpInfoPane `\x1E` → cpGrayDialog `0x1E`=`0x3D` → cpAppColor[`0x3D`]=`0x13`, cyan on blue). D7/D8/D12. |
| 79 ✅ | `TFileDialog` | `TDialog` | `tfildlg.cpp`, `sfildlg.cpp`, `nmfildlg.cpp` | `dialog` (filedlg) | MECHANICAL | **✅** `FileDialog`, a D2 embed-delegate over `Dialog`, assembling 76+77+78 + labels + `History` + `ScrollBar` + buttons (faithful order/bounds/growMode; `button_specs()` pure helper). **Landed in two stages:** **B1** = skeleton (assembly; `handle_event` cmFileOpen/Replace/Clear→`end_modal`, cmFileDoubleClicked→cmOK; `size_limits` 49×19; initial `readDirectory` mapped to a guarded `reset_current` override — the ctx-bearing hook the modal loop runs once before the first draw, since the ctor has no ctx — driving `FileList::read_directory` + owner→child `set_dir_info` via the new `pub(crate) Dialog::child_mut`; `read_directory_listing` ctx-free split). **B2** = `valid()` (faithful 4-branch navigate/accept: cmCancel/cmFileClear→true, isWild/isDir→navigate (re-read, keep open)→false, validFileName→accept, else→invalidFile box), `getFileName` + the **D14 lexical path helpers** (`expand_path` fexpand, `is_wild`/`is_dir`/`is_dir_only`/`split_dir_file`/`path_valid`/`valid_file_name` — std::path, no canonicalize), `checkDirectory` + both error boxes via the **async-modal-from-view** seam (Informational, no pump change), `value`/`set_value` (D10). **Still breadcrumbed:** the screen-relative resize block, `wfGrow`, and the `FileEditor::saveAs` consumer (now unblocked by `value()`). |
| 80 | `TChDirDialog` | `TDialog` | `tchdrdlg.cpp`, `schdrdlg.cpp`, `nmchdrdl.cpp` | `dialog` (filedlg) | MECHANICAL | owns `TInputLine`+`TDirListBox`(→75)+buttons |
| 81 | `TColorItem`/`TColorGroup`/`TColorIndex` | — | `colorsel.h` (inline), `sclrsel.cpp` | `dialog` (colordlg) | MECHANICAL | color-selection data tree |
| 82 | `TColorSelector` | `TView` | `colorsel.cpp`, `sclrsel.cpp` | `dialog` (colordlg) | MECHANICAL | 16-color grid |
| 83 | `TColorDisplay` | `TView` | `colorsel.cpp`, `sclrsel.cpp` | `dialog` (colordlg) | MECHANICAL | sample text preview |
| 84 | `TMonoSelector` | `TCluster` | `colorsel.cpp`, `sclrsel.cpp` | `dialog` (colordlg) | MECHANICAL | mono attribute cluster |
| 85 | `TColorGroupList` | `TListViewer` | `colorsel.cpp`, `sclrsel.cpp` | `dialog` (colordlg) | MECHANICAL | owns `TColorGroup`s (→81) |
| 86 | `TColorItemList` | `TListViewer` | `colorsel.cpp`, `sclrsel.cpp` | `dialog` (colordlg) | MECHANICAL | owns `TColorItem`s (→81) |
| 87 | `TColorDialog` | `TDialog` | `sclrsel.cpp`/`colorsel.cpp` | `dialog` (colordlg) | MECHANICAL | owns 82–86 + `TLabel`s; edits a `TPalette` |
| 88 | `TNode` | — | `outline.h` (inline), `soutline.cpp` | `widgets::outline` | MECHANICAL | tree node (text/children/expanded) |
| 89 | `TOutlineViewer` | `TScroller` | `toutline.cpp`, `soutline.cpp` | `widgets::outline` | FOUNDATION | abstract tree walker; line glyphs (D7) |
| 90 | `TOutline` | `TOutlineViewer` | `toutline.cpp`, `soutline.cpp`, `nmoutlin.cpp` | `widgets::outline` | MECHANICAL | concrete `TNode`-backed outline (→88) |
| 91 | `TTextDevice` | `TScroller` | `textview.cpp` | `widgets::terminal` | MECHANICAL | abstract scrollable text sink (was `streambuf`) |
| 92 | `TTerminal` | `TTextDevice` | `textview.cpp` | `widgets::terminal` | MECHANICAL | ring-buffer terminal view |

---

## Parallelizable batches

Independent siblings that can be handed to concurrent workers once their shared
prerequisite is done.

- **Batch A — after `TView` (23):** `TScrollBar` (25) and `TBackground` (29) are
  independent. `TScroller`/`TListViewer` then both depend only on `TScrollBar`.
- **Batch B — after `TDialog` (34) + `TView`/`TCluster`/`TStaticText`:** the
  Phase-3 leaves split into two independent waves:
  - *No-validator wave (parallel):* `TStaticText` (36), `TButton` (37),
    `TIndicator` (45), then `TParamText`/`TLabel` (40/41, need 36),
    `TCheckBoxes`/`TRadioButtons`/`TMultiCheckBoxes` (42/43/44, need `TCluster` 38).
  - *Validator wave:* `TValidator` (35) → `TInputLine` (39).
  These two waves share no state and can run concurrently.
- **Batch C — concrete validators (58–62):** all depend only on `TValidator`
  (35); fully parallel among themselves.
- **Batch D — after `TMenuView` (49):** `TMenuBar` (50) and `TMenuBox` (51) are
  independent siblings (`TMenuPopup` 52 then needs `TMenuBox`).
- **Batch E — Phase-5 dialog families:** once `TDialog`, `TListViewer`,
  `TInputLine`, `TButton` exist, the **color dialog** (81–87), **file/chdir
  dialog** (71–80), **editor** (66–69), **outline** (88–90), and **textview**
  (91–92) families are mutually independent and can be assigned to separate
  workers in parallel.

---

## Typical view-class deviation set

Most `TView` subclasses touch **D1** (naming/`tv::`), **D2** (trait + `ViewState`
composition), **D4** (event `enum` + match), **D5** (state/options bool structs),
**D6** (typed `Style`/`Color`), **D7** (`ctx.theme.style(Role::…)` + glyphs), and
**D8** (draw into back-buffer; no per-write occlusion). Add **D10** if the view
carries data (`getData`/`setData` → typed `value`/`set_value`), **D13** if it
renders text, and — for **container** classes (`TGroup` and descendants) — **D3**
(`ViewId` handles + downward `Context`) and **D9** (`execView`/modal/drag become
capture-stack handlers, not nested loops).
