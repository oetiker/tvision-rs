# TDirListBox  (guide pp. 418–419)

Rust module(s): src/dialog/filedlg.rs   |   magiblot: include/tvision/stddlg.h / source/tvision/tdirlist.cpp

> The guide entry (p. 418–419) is terse: "Details of TDirListBox's fields and
> methods are in the online Help." The full surface is in stddlg.h +
> tdirlist.cpp, which document: field `dir`, field `cur`, and methods
> `getText`, `handleEvent` (inherited from `TListBox`), `isSelected`,
> `newDirectory`, `setState`.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `dir` (field) | 418 | PORTED | OK | `DirListBox::dir: String` | 3 | Private field. Internal comment raised: explains it is informational only and that the dir-change flow reads `focused_entry().dir()` instead. |
| `cur` (field) | 418 | PORTED | OK | `DirListBox::cur: usize` | 3 | Private field. Internal comment raised: explains the `is_selected` override and that the C++ equivalent was `ushort cur`. |
| `getText` (method) | 418 | PORTED | OK | `DirListBox::get_text(item: i32) -> String` (impl `ListViewer::get_text`) | 3 | Raised: doc now explains the glyph prefix in the returned text and advises using `focused_entry()` for a bare path. |
| `handleEvent` (method) | 418 | EQUIVALENT | OK | `DirListBox::handle_event` delegates to `list_viewer::handle_event` | 3 | Raised: doc now explains the events handled (arrows, Page Up/Down, mouse) and the double-click→`CHANGE_DIR` flow. |
| `isSelected` (method) | 418 | PORTED | OK | `DirListBox::is_selected(item: i32) -> bool` (impl `ListViewer::is_selected`) | 3 | Raised: doc now explains the override rationale (ancestor highlight separate from cursor) and how `cur` is set/moved. |
| `newDirectory` (method) | 418 | PORTED | OK | `DirListBox::new_directory(dir: &str, ctx: &mut Context)` | 3 | Already at score 3. |
| `setState` (method) | 418 | PORTED | OK | `DirListBox::set_state` (impl `View::set_state`) | 3 | Already at score 3. |
| `showDrives` (private method) | — | NOT-PORTED | — | — | — | DOS drive enumeration. Native-path deviation (D14) explicitly drops it. |
| `showDirs` (private method) | — | EQUIVALENT | OK | `DirListBox::build_tree(dir, subdirs) -> (Vec<DirEntry>, usize)` (private) | 3 | Already at score 3. |

## Summary

- PORTED: 6   EQUIVALENT: 2   NOT-PORTED: 1   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- All 4 previously below-bar public symbols (`get_text`, `handle_event`, `is_selected`, plus `list` and `new`) raised to score 3 in this pass. Private fields `dir` and `cur` received improved internal comments.
