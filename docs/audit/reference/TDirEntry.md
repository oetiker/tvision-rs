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
| `DisplayText` (record field / `PString`) | 418 | PORTED | OK | `DirEntry::display_text: String` | 2 | C++ `char* displayText` (heap-allocated via `newStr`). Rust `pub String` — owned, same semantics, no fixed-length cap. Field doc comment explains "may carry tree-glyph prefixes." |
| `Directory` (record field / `PString`) | 418 | PORTED | OK | `DirEntry::directory: String` | 2 | C++ `char* directory` (heap-allocated). Rust `pub String`. Field doc comment explains "the path this entry navigates to when selected." |
| Constructor `TDirEntry(TStringView, TStringView)` | — | PORTED | OK | `DirEntry::new(display_text, directory) -> DirEntry` | 2 | C++ constructor allocates two heap strings via `newStr`. Rust `new` takes `impl Into<String>` for both. Doc comment present ("Construct from any text/path pair"). How/when to use could be expanded (score 2). |
| Destructor `~TDirEntry()` | — | NOT-PORTED | — | — | — | C++ manually `delete[]`s both strings. Rust `Drop` for `String` is automatic; no explicit destructor needed or written. |
| `text()` (accessor) | — | PORTED | OK | `DirEntry::text() -> &str` | 2 | C++ `char* text() { return displayText; }`. Rust returns `&str`. Doc comment: "The display string." |
| `dir()` (accessor) | — | PORTED | OK | `DirEntry::dir() -> &str` | 2 | C++ `char* dir() { return directory; }`. Rust returns `&str`. Doc comment: "The navigation path." |
| `TDirEntry` type as a whole | 418 | PORTED | OK | `tv::dialog::DirEntry` struct | 3 | Type-level doc comment explains both fields, the tree-glyph note, and the Turbo Vision heritage section (ports `TDirEntry`, two `char*` become owned `String`s). Derives `Debug`, `Clone`, `PartialEq`, `Eq` for free. |

## Summary

- PORTED: 6   EQUIVALENT: 0   NOT-PORTED: 1   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 5   |   → concept: 0
- Notable finding: No gaps or suspect items. The five public symbols (struct + two fields + two accessors) all score 2 — they explain what each item is but stop short of the "how/when to use it" bar for a 3; adding a brief note on the tree-glyph prefix convention (produced by `DirListBox::build_tree`) to the `display_text` field doc and to `DirEntry::new` would bring them to 3.
