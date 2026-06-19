# TDirEntry  (guide p. 418)

Rust module(s): src/dialog/filedlg.rs   |   magiblot: include/tvision/stddlg.h

> Guide (p. 418): "TDirEntry is a simple record type holding directory path
> strings and descriptions. These records are used in TDirCollection objects to
> hold directory information for the change-directory dialog box."
> Declaration: `TDirEntry = record { DisplayText: PString; Directory: PString; }`
> In stddlg.h it is a class with two `char*` fields and accessor methods
> `text()` / `dir()`, plus a constructor and destructor managing heap strings.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `DisplayText` (record field / `PString`) | 418 | PORTED | OK | `DirEntry::display_text: String` | 3 | Raised: doc now explains tree-glyph prefix convention from `build_tree` and advises against setting directly when `new_directory` is used. |
| `Directory` (record field / `PString`) | 418 | PORTED | OK | `DirEntry::directory: String` | 3 | Raised: doc now explains this is the path navigated on selection and how `ChDirDialog::handle_event` uses it via `focused_entry`. |
| Constructor `TDirEntry(TStringView, TStringView)` | — | PORTED | OK | `DirEntry::new(display_text, directory) -> DirEntry` | 3 | Raised: doc now explains custom-entry use case vs. `build_tree`-produced entries. |
| Destructor `~TDirEntry()` | — | NOT-PORTED | — | — | — | C++ manually `delete[]`s both strings. Rust `Drop` for `String` is automatic; no explicit destructor needed or written. |
| `text()` (accessor) | — | PORTED | OK | `DirEntry::text() -> &str` | 3 | Raised: doc now notes it includes tree-glyph prefixes and advises using `dir()` to get a bare path. |
| `dir()` (accessor) | — | PORTED | OK | `DirEntry::dir() -> &str` | 3 | Raised: doc now notes who reads it (`ChDirDialog::handle_event` via `focused_entry`) and absence of trailing `/`. |
| `TDirEntry` type as a whole | 418 | PORTED | OK | `tv::dialog::DirEntry` struct | 3 | Type-level doc comment explains both fields, the tree-glyph note, and the Turbo Vision heritage section. |

## Summary

- PORTED: 6   EQUIVALENT: 0   NOT-PORTED: 1   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- All 5 previously below-bar public symbols raised to score 3 in this pass.
