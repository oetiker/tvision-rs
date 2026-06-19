# TFileInfoPane  (guide p. 441)

Rust module(s): `src/dialog/filedlg.rs`   |   magiblot: `include/tvision/stddlg.h` / `source/tvision/tfildlg.cpp`

> The 1992 print guide gives only a brief stub for `TFileInfoPane` (p. 441:
> "TFileInfoPane represents a file information pane … Details of TFileInfoPane's
> fields and methods are in the online Help."). The authoritative specification
> is `stddlg.h`.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `draw` (method) | 441 | PORTED | OK | `FileInfoPane::draw` (impl `View::draw`) | 3 | Already at score 3. |
| `handleEvent` (method) | 441 | PORTED | OK | `FileInfoPane::handle_event` (impl `View::handle_event`) | 3 | Already at score 3. |
| `getPalette` (method) | 441 | EQUIVALENT | OK | `Role::InfoPane` in `Theme` | 3 | Documented in `src/theme.rs` (theme pass): `Role::InfoPane` now describes the widget (`FileInfoPane::draw`), the color (cyan on blue, `0x13`), and the chain (`cpInfoPane[1]=0x1E → cpGrayDialog[30]=0x3D → cpAppColor[61]=0x13`). |
| `file_block` (private field, `TSearchRec`) | impl | PORTED | OK | `FileInfoPane.file_block: Option<SearchRec>` | 3 | Private field. Internal comment raised: explains `None` = blank draw, update path through `ResolveFocusedFile` broker, and C++ comparison. |
| `directory` (Rust-only cached field) | impl | EQUIVALENT | OK | `FileInfoPane.directory: String` | 3 | Private field. Internal comment raised: explains the D3 caching rationale and the `set_dir_info` refresh path. |
| `wild_card` (Rust-only cached field) | impl | EQUIVALENT | OK | `FileInfoPane.wild_card: String` | 3 | Private field. Internal comment raised: same caching rationale as `directory`. |
| `on_file_focused` (Rust-only method) | impl | EQUIVALENT | OK | `FileInfoPane::on_file_focused` | 3 | Already at score 3. |
| `set_dir_info` (Rust-only method) | impl | EQUIVALENT | OK | `FileInfoPane::set_dir_info` | 3 | Already at score 3. |
| `months` / `amText` / `pmText` (static data) | impl | PORTED | OK | `MONTHS: [&str; 13]`, `AM`, `PM` constants | 2 | Private constants — not held to public bar. Doc comments adequate for internal use. |
| `pack_dos_time` helper | impl | EQUIVALENT | OK | `fn pack_dos_time(t: &SystemTime) -> i32` | 3 | Already at score 3 (private). |
| `DOTDOT_TIME` constant | impl | EQUIVALENT | OK | `const DOTDOT_TIME: i32 = 0x0021_0000` | 2 | Private constant — not held to public bar. Doc comment adequate for internal use. |
| `new` (constructor) | impl | PORTED | OK | `FileInfoPane::new(bounds, directory, wild_card) -> FileInfoPane` | 3 | Raised: doc now explains the D3 caching rationale, that `file_block` starts `None`, and how/when the pump fills it. |
| `Load` / `Store` / `streamableName` | impl | NOT-PORTED | — | — | — | `TStreamable` persistence — dropped per D12. |

## Summary

- PORTED: 4   EQUIVALENT: 7   NOT-PORTED: 1   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- `getPalette` → `Role::InfoPane` raised to 3 in the theme.rs Role pass. `new` raised to score 3. Private fields `file_block`, `directory`, `wild_card` received improved internal comments.
