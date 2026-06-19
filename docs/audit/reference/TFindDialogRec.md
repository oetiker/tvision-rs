# TFindDialogRec type  (guide p. 443)

Rust module(s): src/widgets/editor.rs + src/app/program.rs   |   magiblot: include/tvision/editors.h + source/tvision/teditor1.cpp

> `TFindDialogRec` is a plain-old-data record passed to the `EditorDialog` callback
> (constant `edFind`) to carry the search string and option flags between the Find
> dialog and the editor. Its two fields map directly to what the Rust port stores
> per-instance on `Editor` plus the `EF_*` constants; the dialog itself is built
> inline in the pump's deferred handler rather than through a function-pointer
> callback.
>
> **C++ declaration** (`include/tvision/editors.h`, lines 553–562):
> ```cpp
> struct TFindDialogRec {
>     char find[maxFindStrLen];   // maxFindStrLen = 80 (config.h)
>     ushort options;
> };
> ```
> The record was populated by the application-supplied `EditorDialog` function
> (passing `edFind`) and then read back by `TEditor::find()` in `teditor1.cpp`
> (lines 478–484).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Find` field (`char[80]`) | 443 | EQUIVALENT | OK | `Editor.find_str: String` (private, accessible via `find_str()` / `set_find_str()`) | N/A | C++ `find[maxFindStrLen]` (80-byte fixed array on a **class-static** field `TEditor::findStr[80]`, shared across all editor instances). Rust stores the search string per-instance as an owned `String` with no hard length cap at the model layer; the dialog `InputLine` is constructed with `max_len=81` (`LimitMode::MaxBytes`), enforcing the C++ 80-char limit at the UI seam. The static→per-instance change is a deliberate idiomatic deviation (no global mutable state); it is consistent with how `editorFlags` and `replaceStr` are also moved per-instance. Idiomatic mapping: fixed C-string array → `String`. |
| `Options` field (`ushort`) | 443 | EQUIVALENT | OK | `Editor.editor_flags: u16` (bits `EF_CASE_SENSITIVE \| EF_WHOLE_WORDS_ONLY`; only bits 0–1 are read back from the Find dialog) | N/A | C++ `options: Word` carries a combination of `efXXXX` flag constants; the Find dialog fills only `efCaseSensitive` (0x0001) and `efWholeWordsOnly` (0x0002). Rust maps these to `EF_CASE_SENSITIVE` / `EF_WHOLE_WORDS_ONLY` constants (editor.rs lines 87–89) with the same numeric values. The options word is stored as `editor_flags: u16` on `Editor`, also per-instance (matching the per-instance move of `find`). Idiomatic mapping: flag word → struct-of-bools is listed in the README as an expected mapping, but here the Rust code intentionally keeps the raw `u16` bitfield to allow passing it directly to `Editor::search(opts)` unchanged. `EF_DO_REPLACE` (0x0010) and `EF_REPLACE_ALL` (0x0008) live in the same word but are only used by the Replace dialog; the Find dialog completion masks them off (`& 0x0003`). |
| Constructor (`TFindDialogRec(str, flags)`) | 443 | EQUIVALENT | OK | `Deferred::OpenFindDialog { editor_id }` → pump builds the dialog and pre-fills from `editor.find_str()` / `editor.editor_flags()` | N/A | C++: `TFindDialogRec findRec(findStr, editorFlags)` constructs the record in `TEditor::find()` before calling `editorDialog(edFind, &findRec)`. Rust: the deferred pump handler (`program.rs` lines 2579–2663) reads the same two fields from the live `Editor` (via downcast) and seeds the `InputLine` and `CheckBoxes` controls of the dialog. The construct-then-pass idiom collapses into the pre-fill step at dialog build time. Idiomatic mapping: POD record + function-pointer callback → typed deferred effect + pump-built dialog. |
| Record as value protocol (getData/setData) | 443 | EQUIVALENT | OK | `ModalCompletion::FindPick { editor_id, find_id, opts_id }` completion reads `InputLine.value()` + `CheckBoxes.cluster.value`, writes back via `set_find_str` / `set_editor_flags` | N/A | C++: after `editorDialog` returns (non-cancel), `TEditor::find()` does `strcpy(findStr, findRec.find); editorFlags = findRec.options & ~efDoReplace` (teditor1.cpp lines 481–482). Rust: `ModalCompletion::FindPick` at program.rs lines 3044–3076 reads the dialog controls by `ViewId`, applies the same mask (`& 0x0003`), and calls the per-instance setters. The record fields play the role of a `getData` result — idiomatic mapping: `getData`/`setData` → the D10 value protocol. |

## Summary

- PORTED: 0   EQUIVALENT: 4   NOT-PORTED: 0   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0 (all symbols are private; N/A)   |   → concept: 0
- Notable finding: The most important design point is that C++ stores `findStr[80]`,
  `replaceStr[80]`, and `editorFlags` as **class-static** fields of `TEditor`
  (shared across all editor instances in the process), while Rust makes all three
  **per-instance** fields on `Editor`. This is a deliberate, idiomatic deviation
  (no global mutable state) that is consistent throughout the codebase. The change
  is not currently called out explicitly anywhere in the source comments or
  `docs/PORTING-GUIDE.md`; a one-line note in the `find_str` / `editor_flags`
  field comments would make the deviation discoverable.
