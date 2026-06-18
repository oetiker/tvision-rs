# TFileInfoPane  (guide p. 441)

Rust module(s): `src/dialog/filedlg.rs`   |   magiblot: `include/tvision/stddlg.h` / `source/tvision/tfildlg.cpp`

> The 1992 print guide gives only a brief stub for `TFileInfoPane` (p. 441:
> "TFileInfoPane represents a file information pane … Details of TFileInfoPane's
> fields and methods are in the online Help."). The authoritative specification
> is `stddlg.h`.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `draw` (method) | 441 | PORTED | OK | `FileInfoPane::draw` (impl `View::draw`) | 3 | Line 0: `directory + wildcard` path. Line 1 (when a named file is focused): name, size at `w-38`, date as `Mon DD, YYYY HH:MMa/p` in right columns. Remaining rows cleared. All text in `Role::InfoPane`. Matches C++ draw layout. Date formats UTC (no timezone crate); documented in `pack_dos_time`. |
| `handleEvent` (method) | 441 | PORTED | OK | `FileInfoPane::handle_event` (impl `View::handle_event`) | 3 | Subscribes to `Command::FILE_FOCUSED` broadcast; requests `ResolveFocusedFile` broker. No not-selected guard (unlike `FileInputLine` — pane always updates). Functionally matches C++ (which calls `message` then caches the record). |
| `getPalette` (method) | 441 | EQUIVALENT | OK | `Role::InfoPane` in `Theme` | 2 | C++ palette: 1 entry (`Normal`). Rust: `ctx.style(Role::InfoPane)` at draw time. Known idiomatic mapping: class Palette → `tv::Theme`. |
| `file_block` (private field, `TSearchRec`) | impl | PORTED | OK | `FileInfoPane.file_block: Option<SearchRec>` | 2 | C++: always-set `TSearchRec`; empty name = blank draw. Rust: `Option<SearchRec>`; `None` = blank. Same semantics, more idiomatic. |
| `directory` (Rust-only cached field) | impl | EQUIVALENT | OK | `FileInfoPane.directory: String` | 2 | No C++ field — C++ `draw` reads `owner->directory` directly. Rust: cached copy pushed by `set_dir_info` because a child cannot read its owner (D3). |
| `wild_card` (Rust-only cached field) | impl | EQUIVALENT | OK | `FileInfoPane.wild_card: String` | 2 | Same rationale as `directory`. C++ reads `owner->wildCard` in `draw`; Rust caches it. |
| `on_file_focused` (Rust-only method) | impl | EQUIVALENT | OK | `FileInfoPane::on_file_focused` | 3 | No C++ equivalent — the broker seam (D3/D4) requires this entry point for the pump to deliver the resolved record. C++ inlined the `file_block` assignment in `handleEvent`'s broadcast path via `infoPtr`. |
| `set_dir_info` (Rust-only method) | impl | EQUIVALENT | OK | `FileInfoPane::set_dir_info` | 3 | No C++ equivalent — cache refresh when `FileDialog` re-reads with a new directory/wildcard. C++ `draw` read from `owner` directly. |
| `months` / `amText` / `pmText` (static data) | impl | PORTED | OK | `MONTHS: [&str; 13]`, `AM`, `PM` constants | 2 | Same month-name array (1-indexed, index 0 = `""`), same `"a"`/`"p"` suffix strings. |
| `pack_dos_time` helper | impl | EQUIVALENT | OK | `fn pack_dos_time(t: &SystemTime) -> i32` | 3 | C++ used `findfirst`/`ffblk` which filled the DOS `time` field natively. Rust packs `std::time::SystemTime` into the DOS `ftime` bitfield using Howard Hinnant's civil-from-days algorithm — same wire format, different source. UTC display (no timezone) is documented. |
| `DOTDOT_TIME` constant | impl | EQUIVALENT | OK | `const DOTDOT_TIME: i32 = 0x0021_0000` | 2 | No C++ equivalent — C++ statted the `..` entry or left its time zero. Rust synthesizes `..` without statting the parent; this constant gives it a well-formed date (Jan 01 1980 00:00) rather than a blank display. Extension required by native-path deviation D14. |
| `Load` / `Store` / `streamableName` | impl | NOT-PORTED | — | — | — | `TStreamable` persistence — dropped per D12. |

## Summary

- PORTED: 4   EQUIVALENT: 7   NOT-PORTED: 1   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 3   |   → concept: 0
- Notable findings: No gaps or suspect items. The most structurally interesting deviation is the owner-field caching (`directory`, `wild_card`) required because Rust children cannot read their parent (D3) — documented in the module doc and in each field's rustdoc. The `pack_dos_time` helper is a non-trivial addition required by the native-path deviation (D14): it converts `std::fs` mtimes into the same DOS `ftime` wire format the `draw` method unpacks, keeping the draw logic faithful to the C++ while working without DOS findfirst.
