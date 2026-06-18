# TReplaceDialogRec type  (guide p. 519)

Rust module(s): src/widgets/editor.rs + src/app/program.rs   |   magiblot: include/tvision/editors.h + source/tvision/teditor2.cpp

> `TReplaceDialogRec` is a plain-old-data record passed to the `EditorDialog`
> callback (constant `edReplace`) to carry the search string, replacement string,
> and option flags between the Replace dialog and the editor. Its three fields map
> to `Editor.find_str`, `Editor.replace_str`, and `Editor.editor_flags` (bits 0–3),
> all stored per-instance in the Rust port.
>
> **C++ declaration** (`include/tvision/editors.h`, lines 567–586):
> ```cpp
> struct TReplaceDialogRec {
>     char find[maxFindStrLen];      // maxFindStrLen    = 80 (config.h)
>     char replace[maxReplaceStrLen]; // maxReplaceStrLen = 80 (config.h)
>     ushort options;
> };
> ```
> The record was populated by the application-supplied `EditorDialog` function
> (passing `edReplace`) and then read back by `TEditor::replace()` in
> `teditor2.cpp` (lines 364–372).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Find` field (`char[80]`) | 519 | EQUIVALENT | OK | `Editor.find_str: String` (private; `find_str()` / `set_find_str()`) | N/A | Same mapping as `TFindDialogRec.find` — see TFindDialogRec.md. C++ class-static `TEditor::findStr[80]` → per-instance `String`. The Replace dialog `InputLine` for the search string uses `max_len=81` (`LimitMode::MaxBytes`), enforcing the 80-char cap at the UI seam (program.rs line 2698). |
| `Replace` field (`char[80]`) | 519 | EQUIVALENT | OK | `Editor.replace_str: String` (private; `replace_str()` / `set_replace_str()`) | N/A | C++ `replace[maxReplaceStrLen]` on the **class-static** `TEditor::replaceStr[80]`. Rust uses a per-instance `String` for the same idiomatic reasons as `find_str` (no global mutable state). The Replace dialog second `InputLine` uses `max_len=81` (program.rs line 2719), mirroring the 80-char C++ cap. Idiomatic mapping: fixed C-string array → `String`. |
| `Options` field (`ushort`) | 519 | EQUIVALENT | OK | `Editor.editor_flags: u16` (bits `EF_CASE_SENSITIVE \| EF_WHOLE_WORDS_ONLY \| EF_PROMPT_ON_REPLACE \| EF_REPLACE_ALL`; bits 0–3 are read back from the Replace dialog; `EF_DO_REPLACE` is added unconditionally by the completion) | N/A | C++ `options: Word` carries `efCaseSensitive` (0x0001), `efWholeWordsOnly` (0x0002), `efPromptOnReplace` (0x0004), `efReplaceAll` (0x0008), and — set by `replace()` — `efDoReplace` (0x0010). Rust maps these verbatim as `EF_*` constants with the same bit values (editor.rs lines 87–96). The Replace dialog `CheckBoxes` presents all four user-visible flags (program.rs lines 2742–2748); the completion masks `& 0x000F` (bits 0–3) and then ORs in `EF_DO_REPLACE` unconditionally (program.rs line 3115) — matching `editorFlags = replaceRec.options | efDoReplace` in `TEditor::replace()` (teditor2.cpp line 371). Flag names, bit positions, and the `|efDoReplace` assignment are all faithful. |
| Constructor (`TReplaceDialogRec(str, rep, flags)`) | 519 | EQUIVALENT | OK | `Deferred::OpenReplaceDialog { editor_id }` → pump pre-fills from `editor.find_str()`, `editor.replace_str()`, `editor.editor_flags()` | N/A | C++: `TReplaceDialogRec replaceRec(findStr, replaceStr, editorFlags)` in `TEditor::replace()` (teditor2.cpp line 366) before calling `editorDialog(edReplace, &replaceRec)`. Rust: pump handler (program.rs lines 2666–2778) reads all three fields from the live `Editor` (downcast) and seeds two `InputLine` controls and the `CheckBoxes`. Pre-fill bit mask is `editor_flags & 0x000F` (bits 0–3 only, stripping `EF_DO_REPLACE` which is the internal "is-replace" sentinel, not a user-visible checkbox). Matches the C++ ctor seeding. Idiomatic mapping: POD record + function-pointer callback → typed deferred effect + pump-built dialog. |
| Record as value protocol (getData/setData) | 519 | EQUIVALENT | OK | `ModalCompletion::ReplacePick { editor_id, find_id, replace_id, opts_id }` completion reads both `InputLine` values + `CheckBoxes.cluster.value`, writes back via `set_find_str` / `set_replace_str` / `set_editor_flags(opts \| EF_DO_REPLACE)` | N/A | C++: after non-cancel return, `TEditor::replace()` does `strcpy(findStr, replaceRec.find); strcpy(replaceStr, replaceRec.replace); editorFlags = replaceRec.options | efDoReplace` (teditor2.cpp lines 369–371). Rust: `ModalCompletion::ReplacePick` at program.rs lines 3081–3117 reads three dialog controls by `ViewId`, applies the same mask and `|EF_DO_REPLACE`, and calls the per-instance setters. Faithful field-for-field. Idiomatic mapping: `getData`/`setData` → D10 value protocol. |

## Summary

- PORTED: 0   EQUIVALENT: 5   NOT-PORTED: 0   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0 (all symbols are private; N/A)   |   → concept: 0
- Notable finding: All three C++ fields (`find`, `replace`, `options`) and the
  `efDoReplace`-or assignment are faithfully reproduced. The `options` bit mask
  handled by the Replace dialog is wider (bits 0–3) than for the Find dialog (bits
  0–1), and `EF_DO_REPLACE` (0x0010) is always ORed in post-completion, exactly
  matching the C++ `replace()` body. No gaps or suspect items. The same
  static→per-instance deviation noted in TFindDialogRec.md applies to
  `replace_str` as well; it is the single undocumented-in-source deviation across
  both records.
