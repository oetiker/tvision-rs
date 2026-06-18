# TSearchRec type  (guide p. 530)

Rust module(s): `src/dialog/filedlg.rs`   |   magiblot: `include/tvision/stddlg.h`

> The 1992 print guide documents `TSearchRec` on p. 530 as a Turbo Pascal record
> type holding the fields returned by DOS `FindFirst`/`FindNext`. The magiblot
> C++ definition in `stddlg.h` is:
> ```c
> struct TSearchRec { uchar attr; int32_t time; int32_t size; char name[MAXFILE+MAXEXT-1]; };
> ```

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TSearchRec` record type | 530 | PORTED | OK | `pub struct SearchRec` | 3 | C++: POD struct populated by DOS `findfirst`/`findnext`. Rust: owned struct populated by `FileList::raw_from_fs` + `build_listing`. Fields `attr`/`time`/`size`/`name` all present. |
| `attr` field (`uchar`, DOS file-attribute byte) | 530 | PORTED | OK | `SearchRec.attr: u8` | 3 | C++: `uchar` (1 byte). Rust: `u8`. Only `FA_DIREC` (`0x10`) is examined in this port (no archive/hidden/system bits used). Correctly noted in the field doc. |
| `time` field (`int32_t`, packed DOS timestamp) | 530 | EQUIVALENT | OK | `SearchRec.time: i32` (packed DOS `ftime` via `pack_dos_time`) | 3 | C++: filled by DOS `findfirst` in native DOS `ftime` bitfield format. Rust: filled by `pack_dos_time(std::fs::Metadata::modified())` — same bitfield layout, different source. The display (UTC, not local) is documented. `EQUIVALENT` because the wire format is identical but the population path is entirely re-implemented for native Linux. |
| `size` field (`int32_t`, file size in bytes) | 530 | PORTED | OK | `SearchRec.size: i32` | 2 | C++: `int32_t` from DOS findfirst. Rust: `meta.len().min(i32::MAX as u64) as i32` (saturated cast). Saturation for files > 2 GiB is documented in `FileList::raw_from_fs`. |
| `name` field (`char[MAXFILE+MAXEXT-1]`, filename) | 530 | EQUIVALENT | OK | `SearchRec.name: String` | 2 | C++: fixed-length C string (14 bytes on DOS: 8.3 format). Rust: owned `String` (no length cap — Linux allows up to 255 bytes per component). The `Clone` derive replaces the need for the struct to be `POD`-copyable. Known: fixed C array → owned `String`. |
| `FA_DIREC` (`= 0x10`) | impl | PORTED | OK | `pub const FA_DIREC: u8 = 0x10` | 2 | DOS `FA_DIREC` attribute bit. Value matches. The only DOS attribute bit used. |
| DOS findfirst / findnext machinery | 530 | NOT-PORTED | — | — | — | The DOS `findfirst`/`findnext` calls that populate `TSearchRec` in C++ have no Linux counterpart. Replaced entirely by `std::fs::read_dir` + `std::fs::metadata` (deviation D14). Module doc explains: "Paths are native and `/`-separated, enumerated with `std::fs::read_dir`. There is no DOS drive-letter machinery." |
| DOS drive-letter / `\`-separator model | 530 | NOT-PORTED | — | — | — | No Linux counterpart. Deviation D14. |
| DOS `attr` bits other than `FA_DIREC` (archive, hidden, system, read-only, volume) | 530 | NOT-PORTED | — | — | — | Only `FA_DIREC` is used by the file dialog logic. The other DOS attribute bits have no behavioral role in the port. `attr` field is kept as `u8` so the bit is testable, but no other bits are set or tested. |

## Summary

- PORTED: 4   EQUIVALENT: 2   NOT-PORTED: 3   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 2   |   → concept: 0
- Notable findings: No gaps or suspect items. The most significant deviation is the `time` field: the DOS `ftime` bitfield is populated by `pack_dos_time` from `std::fs::Metadata::modified()` (UTC, not local time) rather than by DOS findfirst. This is deliberate and documented — the wire format fed to `FileInfoPane::draw` is identical (same bitfield unpack), and the UTC vs. local difference is cosmetic for a file browser. The three NOT-PORTED entries (findfirst, drive model, unused DOS attribute bits) are all valid omissions under deviation D14 (native paths).
