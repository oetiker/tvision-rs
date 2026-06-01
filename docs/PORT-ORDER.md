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
- Color quantization ladder is `mapcolor.cpp` + `palette.cpp`.

**Tags:** `FOUNDATION` (pattern-setting; many deviations collide; careful
first-time work), `MECHANICAL` (leaf/transcription once foundation exists),
`INFRA` (net-new, no C++ source — built per the deviations).

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
| 1 | `TPoint` | — | `objects.h` (inline) | `view` (geometry) | FOUNDATION | x/y; arithmetic ops |
| 2 | `TRect` | — | `objects.h` (inline) | `view` (geometry) | MECHANICAL | a/b corners; intersect/union/move/grow; owns 2×`TPoint` |
| 3 | `TColorRGB`/`TColorDesired` → `Color` | — | `colors.h` (inline) | `color` | FOUNDATION | D6 four-variant enum (Default/Bios/Indexed/Rgb) |
| 4 | `TColorAttr` → `Style` | — | `colors.h` (inline) | `color` | FOUNDATION | D6 fg/bg + `Modifiers` (reverse, no-shadow); owns `Color` |
| 5 | quantization ladder | — | `mapcolor.cpp`, `palette.cpp` | `backend` | INFRA* | D6 RGB→256→16→BIOS faithful port; lives in Backend |
| 6 | `TScreenCell` → `Cell` | — | `scrncell.h` (inline) | `screen` | FOUNDATION | char(s)+`Style`; vendored ratatui cell shape |
| 7 | `TDrawBuffer` | — | `drivers.cpp` | `screen` (`DrawBuffer`) | FOUNDATION | moveStr/moveChar/moveBuf/putAttribute; owns `Cell`s |
| 8 | `TText` | — | `drivers.cpp`, `drivers2.cpp` | `text` | FOUNDATION | D13 width/scroll/cell-writer; `unicode-width`+`-segmentation` |
| 9 | glyph/string tables | — | `tvtext1.cpp`, `tvtext2.cpp` | `theme` (`Glyphs`) | MECHANICAL | D7 frame/scrollbar/marks/icons → `Glyphs` |
| 10 | `TKey` + key events | — | `tkey.cpp`, `tkeys.h` | `event` (`Key`) | FOUNDATION | D1 modern values from crossterm; not BIOS scancodes |
| 11 | `TEvent`/`MouseEventType`/`KeyDownEvent`/`MessageEvent` | — | `tevent.cpp`, `system.h` | `event` | FOUNDATION | D4 `enum Event` sum type; `EventMask` bool struct |
| 12 | `TCommandSet` | — | `tcmdset.cpp` | `command` | FOUNDATION | D1 → `HashSet<Command>`; `Command(u16)` newtype |
| 13 | `TObject` | — | `tobject.cpp` | (absorbed) | FOUNDATION | D2 no root class; lifetime via Rust ownership/`Drop` |
| 14 | `TNSCollection`/`TCollection` | `TObject` | `tcollect.cpp`, `tvobjs.h` | (idiom) | MECHANICAL | → `Vec<T>` + iterators; `firstThat`/`forEach` → iterators |
| 15 | `TNSSortedCollection`/`TSortedCollection` | `TCollection` | `tsortcol.cpp` | (idiom) | MECHANICAL | → `Vec<T: Ord>` |
| 15a | `TStringCollection` | `TSortedCollection` | `tstrcoll.cpp`, `sstrcoll.cpp` | (idiom) | MECHANICAL | → sorted `Vec<String>`; needed by `TStringLookupValidator` (#61) |
| 16 | `Theme` | — | (synthesizes D7 palettes) | `theme` | INFRA | Role→Style map + `Glyphs`; default = classic blue (`cpAppColor`) |
| 17 | `ViewId` arena | — | (replaces `owner`/`current`/`next`) | `view` (`ViewId`) | INFRA | D3 generational index; up/sideways links |
| 18 | renderer back-buffer + diff | — | (replaces `TVWrite`/`drawUnder*`) | `screen` | INFRA | D8 whole-tree redraw + cell diff; vendored ratatui `Buffer` |
| 19 | `Backend` trait + `CrosstermBackend` + `HeadlessBackend` | — | `THardwareInfo`/`TScreen`/`TClipboard` (`tscreen.cpp`, `hardwrvr.cpp`, `tclipbrd.cpp`) as design ref | `backend` | INFRA | D11; size/flush/cursor/clipboard; wraps row 5 ladder |
| 20 | `Clock` + timer queue | — | `TTimerQueue` (`ttimerqu.cpp`) as ref | `timer` | INFRA | D9/D11 injected clock, cancelable handles, poll timeout |
| 21 | capture stack | — | (replaces nested `execView`/`dragView` loops) | `capture` | INFRA | D9 LIFO handlers; modal/drag/press = handlers |
| 22 | `Context` / `DrawCtx` | — | (replaces up-pointers + clip) | `view` | INFRA | D3 downward ctx: theme/clip/parent style; targeted query (D4) |

---

## Phase 1 — Foundation views & program shell

| # | Class | Base | C++ files | Rust module | Tag | Notes / owns |
|---|-------|------|-----------|-------------|-----|--------------|
| 23 | `TView` | `TObject` | `tview.cpp`, `sview.cpp`, `tvexposd.cpp`, `tvcursor.cpp` | `view` | FOUNDATION | D2 `View` trait + `ViewState`; D5 state/options/growMode/dragMode structs; pattern-setting class |
| 24 | `TFrame` | `TView` | `tframe.cpp`, `sframe.cpp`, `framelin.cpp` | `frame` | FOUNDATION | window border/title/icons; glyphs from Theme (D7) |
| 25 | `TScrollBar` | `TView` | `tscrlbar.cpp`, `sscrlbar.cpp` | `widgets::scrollbar` | MECHANICAL | value/min/max/step; `cmScrollBarChanged` broadcast |
| 26 | `TGroup` | `TView` | `tgroup.cpp`, `grp.cpp`, `sgroup.cpp`, `tgrmv.cpp` | `group` | FOUNDATION | D3 owns `Vec<Box<dyn View>>`; D4 three-phase routing; D8 drop buffered/lock; `current` via `ViewId` |
| 27 | `TScroller` | `TView` | `tscrolle.cpp`, `sscrolle.cpp` | `widgets::scroller` | MECHANICAL | takes 2×`TScrollBar` (→25); `delta`/`limit` |
| 28 | `TListViewer` | `TView` | `tlstview.cpp`, `slstview.cpp` | `widgets::listviewer` | FOUNDATION | takes 2×`TScrollBar` (→25); list-render matrix roles (D7); base for list widgets |
| 29 | `TBackground` | `TView` | `tbkgrnd.cpp`, `sbkgrnd.cpp`, `nmbkgrnd.cpp` | `desktop` | MECHANICAL | pattern fill |
| 30 | `TDeskTop` | `TGroup` + `TDeskInit` | `tdesktop.cpp`, `sdesktop.cpp`, `nmdsktop.cpp` | `desktop` | FOUNDATION | owns `TBackground` (→29) via factory mixin; tile/cascade |
| 31 | `TProgram` | `TGroup` + `TProgInit` | `tprogram.cpp` | `app` | FOUNDATION | **factory-mixin deferral:** holds `TStatusLine`/`TMenuBar`/`TDeskTop` via injected factories — those classes are Phase 4 yet `TProgram` precedes them. Owns the single event loop (D9), timer queue (→20). |
| 32 | `TApplication` | `TProgram` + `TAppInit` | `tapplica.cpp` | `app` | MECHANICAL | tile/cascade/dosShell wrappers over `TProgram` |

---

## Phase 2 — Windows & dialogs

| # | Class | Base | C++ files | Rust module | Tag | Notes / owns |
|---|-------|------|-----------|-------------|-----|--------------|
| 33 | `TWindow` | `TGroup` + `TWindowInit` | `twindow.cpp`, `swindow.cpp`, `nmwindow.cpp` | `window` | FOUNDATION | builds `TFrame` (→24) via factory mixin; `standardScrollBar` (→25); zoom/move/close; D2 embed-and-delegate exemplar |
| 34 | `TDialog` | `TWindow` | `tdialog.cpp`, `sdialog.cpp`, `nmdialog.cpp` | `dialog` | FOUNDATION | modal via capture handler (D9); `cmOK`/`cmCancel`; gather/scatter typed values (D10) |

---

## Phase 3 — Simple widgets (mostly independent leaves)

`TInputLine` needs the **abstract `Validator` trait** (row 35) but not the
concrete validators (Phase 5). Split accordingly.

| # | Class | Base | C++ files | Rust module | Tag | Notes / owns |
|---|-------|------|-----------|-------------|-----|--------------|
| 35 | `TValidator` (abstract) | `TObject` | `tvalidat.cpp`, `svalid.cpp` | `validate` | FOUNDATION | D2 `Validator` trait: `is_valid_input`/`is_valid`/`transfer` (D10) |
| 36 | `TStaticText` | `TView` | `tstatict.cpp`, `sstatict.cpp` | `widgets::static_text` | MECHANICAL | word-wrap text draw (D13) |
| 37 | `TButton` | `TView` | `tbutton.cpp`, `sbutton.cpp` | `widgets::button` | MECHANICAL | press animation via Clock (→20); shadow glyphs (D7); broadcast/command flags |
| 38 | `TCluster` | `TView` | `tcluster.cpp`, `scluster.cpp` | `widgets::cluster` | FOUNDATION | owns label strings; base for check/radio; `value`/`enableMask` bits |
| 39 | `TInputLine` | `TView` | `tinputli.cpp`, `sinputli.cpp` | `widgets::input_line` | FOUNDATION | holds optional `Validator` (→35); typed `value`/`set_value` (D10); selection; arrows glyphs (D7) |
| 40 | `TParamText` | `TStaticText` | `tparamte.cpp`, `sparamte.cpp` | `widgets::static_text` | MECHANICAL | printf-style formatted static text |
| 41 | `TLabel` | `TStaticText` | `tlabel.cpp`, `slabel.cpp` | `widgets::label` | MECHANICAL | `link` to a control via `ViewId` (D3); focus-on-shortcut |
| 42 | `TCheckBoxes` | `TCluster` | `tcheckbo.cpp`, `scheckbo.cpp`, `nmchkbox.cpp` | `widgets::cluster` | MECHANICAL | check marks (D7) |
| 43 | `TRadioButtons` | `TCluster` | `tradiobu.cpp`, `sradiobu.cpp`, `nmrbtns.cpp` | `widgets::cluster` | MECHANICAL | radio marks (D7) |
| 44 | `TMultiCheckBoxes` | `TCluster` | `tmulchkb.cpp`, `smulchkb.cpp`, `nmmulchk.cpp` | `widgets::cluster` | MECHANICAL | multi-state marks; `states` array |
| 45 | `TIndicator` | `TView` | `tindictr.cpp`, `editstat.cpp` | `widgets::indicator` | MECHANICAL | editor row/col + modified flag display |

---

## Phase 4 — Lists, menus, status line, history

| # | Class | Base | C++ files | Rust module | Tag | Notes / owns |
|---|-------|------|-----------|-------------|-----|--------------|
| 46 | `TMenuItem`/`TSubMenu`/`TMenu` | — | `menu.cpp` | `menu` | FOUNDATION | menu data tree (name/command/key/submenu); `operator+` builders → Rust builder API |
| 47 | `TStatusItem`/`TStatusDef` | — | `menus.h` (inline) | `status` | MECHANICAL | status-item data (text/key/cmd, help-ctx ranges) |
| 48 | `TListBox` | `TListViewer` | `tlistbox.cpp`, `slistbox.cpp`, `nmlstbox.cpp` | `widgets::list_box` | MECHANICAL | owns a `TCollection` (→14); takes `TScrollBar` (→25); typed value (D10) |
| 49 | `TMenuView` | `TView` | `tmnuview.cpp`, `smnuview.cpp` | `menu` | FOUNDATION | holds `TMenu` (→46); hotkey/shortcut dispatch; `evBroadcast` mask |
| 50 | `TMenuBar` | `TMenuView` | `tmenubar.cpp`, `smenubar.cpp` | `menu` | MECHANICAL | horizontal bar layout |
| 51 | `TMenuBox` | `TMenuView` | `tmenubox.cpp`, `smenubox.cpp` | `menu` | MECHANICAL | vertical popup box; frame glyphs (D7) |
| 52 | `TMenuPopup` | `TMenuBox` | `tmenupop.cpp`, `smenupop.cpp`, `popupmnu.cpp` | `menu` | MECHANICAL | spawns/execs popup (D9); `popupMenu()` free fn in `popupmnu.cpp` |
| 53 | `TStatusLine` | `TView` | `tstatusl.cpp`, `sstatusl.cpp` | `status` | FOUNDATION | owns `TStatusDef`/`TStatusItem` (→47); hint(); help-ctx → hint mapping |
| 54 | history store (`historyAdd`/`Count`/`Str`/`clearHistory`) | — | `histlist.cpp` | `widgets::history` | MECHANICAL | per-id ring buffer backing store |
| 55 | `THistoryViewer` | `TListViewer` | `thstview.cpp` | `widgets::history` | MECHANICAL | reads history store (→54) |
| 56 | `THistoryWindow` | `TWindow` + `THistInit` | `thistwin.cpp` | `widgets::history` | MECHANICAL | owns `THistoryViewer` (→55) via factory mixin |
| 57 | `THistory` | `TView` | `thistory.cpp`, `nmhist.cpp` | `widgets::history` | MECHANICAL | dropdown icon next to `TInputLine` (→39); spawns `THistoryWindow` (→56) |

---

## Phase 5 — Advanced (validators, editors, std dialogs, file/color/outline/text)

| # | Class | Base | C++ files | Rust module | Tag | Notes / owns |
|---|-------|------|-----------|-------------|-----|--------------|
| 58 | `TFilterValidator` | `TValidator` | `tvalidat.cpp` | `validate` | MECHANICAL | char allow-list |
| 59 | `TRangeValidator` | `TFilterValidator` | `tvalidat.cpp` | `validate` | MECHANICAL | numeric min/max; `transfer` (D10) |
| 60 | `TLookupValidator` | `TValidator` | `tvalidat.cpp` | `validate` | MECHANICAL | abstract lookup |
| 61 | `TStringLookupValidator` | `TLookupValidator` | `tvalidat.cpp` | `validate` | MECHANICAL | owns string list |
| 62 | `TPXPictureValidator` | `TValidator` | `tvalidat.cpp` | `validate` | MECHANICAL | Paradox picture-mask state machine |
| 63 | `messageBox`/`messageBoxRect`/`inputBox`/`inputBoxRect` | — | `msgbox.cpp` | `dialog` (msgbox) | MECHANICAL | builds `TDialog`+`TStaticText`+`TButton`(s)/`TInputLine`; result via posted `Command` (D9) |
| 64 | `TStringList`/`TStrListMaker`/`TStrIndexRec` | `TObject` | `tstrlist.cpp`, `sstrlst.cpp` | `text` (resource) | MECHANICAL | string-resource lists; mostly D12-adjacent — minimal port |
| 66 | `TEditor` | `TView` | `teditor1.cpp`, `teditor2.cpp`, `edits.cpp` | `widgets::editor` | FOUNDATION | gap-buffer text editor; takes 2×`TScrollBar`+`TIndicator`; clipboard (D11); search/replace |
| 67 | `TMemo` | `TEditor` | `tmemo.cpp` | `widgets::editor` | MECHANICAL | single-field editor; typed value (D10) |
| 68 | `TFileEditor` | `TEditor` | `tfiledtr.cpp` | `widgets::editor` | MECHANICAL | load/save file backing |
| 69 | `TEditWindow` | `TWindow` | `teditwnd.cpp` | `widgets::editor` | MECHANICAL | owns `TFileEditor` (→68) + scrollbars + `TIndicator` |
| 70 | `TSortedListBox` | `TListBox` | `stddlg.cpp` (member code), `sfilelst.cpp` | `widgets::list_box` | MECHANICAL | incremental-search list; owns `TSortedCollection` |
| 71 | `TDirEntry` | — | `stddlg.h` (inline) | `dialog` (filedlg) | MECHANICAL | dir display/path pair |
| 72 | `TDirCollection` | `TCollection` | `tdircoll.cpp`, `sdircoll.cpp`, `nmdircol.cpp` | `dialog` (filedlg) | MECHANICAL | owns `TDirEntry`s (→71) |
| 73 | `TSearchRec` | — | `stddlg.h` (inline) | `dialog` (filedlg) | MECHANICAL | file metadata record |
| 74 | `TFileCollection` | `TSortedCollection` | `tfilecol.cpp`, `sfilcoll.cpp`, `nmfilcol.cpp` | `dialog` (filedlg) | MECHANICAL | owns `TSearchRec`s (→73) |
| 75 | `TDirListBox` | `TListBox` | `tdirlist.cpp`, `sdirlist.cpp`, `nmdirbox.cpp` | `dialog` (filedlg) | MECHANICAL | owns `TDirCollection` (→72); tree glyphs (D7) |
| 76 | `TFileList` | `TSortedListBox` | `tfillist.cpp`, `sfilelst.cpp`, `nmfillst.cpp` | `dialog` (filedlg) | MECHANICAL | owns `TFileCollection` (→74); reads directory |
| 77 | `TFileInputLine` | `TInputLine` | `sfinputl.cpp` + `stddlg.cpp` (member code; no `t*` file) | `dialog` (filedlg) | MECHANICAL | filename field; reacts to `cmFileFocused` |
| 78 | `TFileInfoPane` | `TView` | `sfinfpne.cpp` + `stddlg.cpp` (member code; no `t*` file) | `dialog` (filedlg) | MECHANICAL | shows focused-file stats |
| 79 | `TFileDialog` | `TDialog` | `tfildlg.cpp`, `sfildlg.cpp`, `nmfildlg.cpp` | `dialog` (filedlg) | MECHANICAL | owns `TFileInputLine`(→77)+`TFileList`(→76)+`TFileInfoPane`(→78)+buttons |
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
