# TFileInputLine  (guide p. 441)

Rust module(s): `src/dialog/filedlg.rs`   |   magiblot: `include/tvision/stddlg.h` / `source/tvision/tfildlg.cpp`

> The 1992 print guide gives only a brief stub for `TFileInputLine` (p. 441:
> "TFileInputLine is a special input line … Details of TFileInfoPane's [sic]
> fields and methods are in the online Help."). The authoritative specification
> is `stddlg.h`.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `handleEvent` (method) | 441 | PORTED | OK | `FileInputLine::handle_event` (impl `View::handle_event`, via `#[delegate]`) | 3 | C++: handles `cmFileFocused` broadcast — if not selected, copies `infoPtr` (the focused `TSearchRec*`) into the field, appending `/<wildCard>` for directories. Rust: runs the inner `InputLine::handle_event` first, then on `FILE_FOCUSED` (if not selected) requests the `ResolveFocusedFile` broker, which later calls `on_file_focused`. Same not-selected guard, same dir-suffix logic; D3/D4 broker seam replaces the inline `infoPtr` read. |
| `wild_card` (Rust-only cached field) | impl | EQUIVALENT | OK | `FileInputLine.wild_card: String` | 2 | No C++ field — C++ `handleEvent` reads `owner->wildCard` directly via the `TFileDialog*` friendship. Rust: cached copy because a child cannot read its owner (D3). Set at construction; refreshed by `set_wild_card` when the dialog re-reads with a new mask. |
| `on_file_focused` (Rust-only method) | impl | EQUIVALENT | OK | `FileInputLine::on_file_focused` | 3 | No direct C++ equivalent — the inline body of the `cmFileFocused` branch in C++ `handleEvent`. Exposed as a method so the pump's `ResolveFocusedFile` broker can call it after downcast. Copies name, appends `/<wildcard>` for directories, resets selection. |
| `set_wild_card` (Rust-only method) | impl | EQUIVALENT | OK | `FileInputLine::set_wild_card` | 2 | No C++ equivalent — cache refresh when `FileDialog::valid`'s wildcard branch changes the mask. C++ read `owner->wildCard` inline; Rust pushes the updated value. |
| `text` accessor (Rust-only) | impl | EQUIVALENT | OK | `FileInputLine::text() -> &str` | 2 | No C++ equivalent — C++ read `fileName->data` directly (public field). Rust exposes a `&str` accessor so `FileDialog::get_file_name` can read the field text without reaching into the inner `InputLine::data`. |
| `as_any_mut` override | impl | EQUIVALENT | OK | `FileInputLine::as_any_mut` returns `self` | 2 | Not in C++ — Rust-specific requirement: the `ResolveFocusedFile` broker must downcast to `FileInputLine` (not to the inner `InputLine`). The comment in the source explicitly explains this is the OPPOSITE of `Memo`'s `as_any_mut`. |
| Embed-and-delegate composition | impl | EQUIVALENT | OK | `#[delegate(to = inner)]` on `View for FileInputLine` | 2 | C++: `TFileInputLine : public TInputLine` (inheritance). Rust: embeds `InputLine`, delegates un-overridden `View` methods via `#[delegate(to = inner)]` (D2). Only `handle_event` and `as_any_mut` differ. `value`/`set_value` forward to the inner `InputLine` (correct — the field text IS the dialog data). |
| `TInputLine::data` (the field text) | impl | PORTED | OK | `FileInputLine.inner.data: String` (via `InputLine`) | 2 | C++: public `data[MAXPATH]`. Rust: `InputLine.data: String`. `on_file_focused` assigns `self.inner.data = text` directly (noted in the comment: `InputLine` exposes no clamping text-setter). |
| `Load` / `Store` / `streamableName` | impl | NOT-PORTED | — | — | — | `TStreamable` persistence — dropped per D12. |

## Summary

- PORTED: 2   EQUIVALENT: 6   NOT-PORTED: 1   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 4   |   → concept: 0
- Notable findings: No gaps or suspect items. The key structural point is the `as_any_mut` override — it must return `self` (not the inner `InputLine`) because the `ResolveFocusedFile` broker downcasts to `FileInputLine`. This is explicitly documented in the source as the deliberate opposite of `Memo`'s pattern. The not-selected guard in `handle_event` correctly matches the C++ invariant that prevents the broadcast from clobbering text the user is actively editing.
