# TDirListBox  (guide pp. 418–419)

Rust module(s): src/dialog/filedlg.rs   |   magiblot: include/tvision/stddlg.h / source/tvision/tdirlist.cpp

> The guide entry (p. 418–419) is terse: "Details of TDirListBox's fields and
> methods are in the online Help." The full surface is in stddlg.h +
> tdirlist.cpp, which document: field `dir`, field `cur`, and methods
> `getText`, `handleEvent` (inherited from `TListBox`), `isSelected`,
> `newDirectory`, `setState`.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `dir` (field) | 418 | PORTED | OK | `DirListBox::dir: String` | 2 | C++ `char dir[MAXPATH]`. Rust uses an owned `String` (no fixed cap; the `/`-termination invariant is documented in the field comment). |
| `cur` (field) | 418 | PORTED | OK | `DirListBox::cur: usize` | 2 | C++ `ushort cur`. Rust `usize`. Field comment explains it indexes the current-directory ancestor entry. |
| `getText` (method) | 418 | PORTED | OK | `DirListBox::get_text(item: i32) -> String` (impl `ListViewer::get_text`) | 2 | C++ `getText(char*, short, short)` fills a buffer. Rust returns an owned `String`. The underlying data source (`DirEntry::display_text`) is the same. |
| `handleEvent` (method) | 418 | EQUIVALENT | OK | `DirListBox::handle_event` delegates to `list_viewer::handle_event` | 2 | C++ inherits `TListBox::handleEvent`. Rust `DirListBox` is a direct `ListViewer` impl (deviation D2) — `handle_event` dispatches to the shared `list_viewer::handle_event` free function. Same event handling semantics (mouse/keyboard navigation). |
| `isSelected` (method) | 418 | PORTED | OK | `DirListBox::is_selected(item: i32) -> bool` (impl `ListViewer::is_selected`) | 2 | C++ `Boolean isSelected(short item) { return item == cur; }`. Rust: `item as usize == self.cur`. Identical logic. |
| `newDirectory` (method) | 418 | PORTED | OK | `DirListBox::new_directory(dir: &str, ctx: &mut Context)` | 3 | C++: fills a `TDirCollection`, calls `newList` + `focusItem(cur)`. Rust: calls `build_tree` (pure) + filesystem read, populates `self.items`, calls `set_range` + `focus_item`. DOS drive-letter model omitted (deviation D14, documented in module doc and `DirListBox` type doc). `build_tree` is separately documented. |
| `setState` (method) | 418 | PORTED | OK | `DirListBox::set_state` (impl `View::set_state`) | 3 | C++: calls `TListBox::setState`, then if `sfFocused` changed, calls `owner->chDirButton->makeDefault(enable)`. Rust: calls `list_viewer::set_state`, then if `StateFlag::Focused` changed and `chdir_button` is `Some`, calls `ctx.make_button_default(btn, enable)`. The owner-downcast-via-raw-pointer pattern becomes the `Deferred::MakeButtonDefault` pump broker (deviation D3, documented in the field and method comments). |
| `showDrives` (private method) | — | NOT-PORTED | — | — | — | DOS drive enumeration (`driveValid`, `getdisk`) has no Linux counterpart. The native-path deviation (D14) explicitly drops the "Drives" entry and all drive-scanning machinery. Documented in module doc: "no DOS drive-letter machinery." |
| `showDirs` (private method) | — | EQUIVALENT | OK | `DirListBox::build_tree(dir, subdirs) -> (Vec<DirEntry>, usize)` (private) | 3 | C++ `showDirs` fills the `TDirCollection` with tree-indented entries using `findFirst/findNext`. Rust splits this into pure `build_tree` + filesystem read in `new_directory`. Semantics identical (root + ancestors + subdirs, glyph fix-up on last entry). Both the tree-layout algorithm and the glyph fix-up are documented inline. |

## Summary

- PORTED: 5   EQUIVALENT: 2   NOT-PORTED: 1   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 4   |   → concept: 0
- Notable finding: The `setState` → `MakeButtonDefault` pump broker is the non-obvious design point — the C++ reaches the owner by raw downcast (`(TChDirDialog*)owner`), while Rust routes the request through the `Deferred` channel. This is documented in the method and field comments; no gap.
