# TPXPictureValidator  (guide pp. 512–514)

Rust module(s): src/validate.rs   |   magiblot: include/tvision/validate.h / source/tvision/tvalidat.cpp

> The Paradox "picture"-mask validator: matches/auto-fills input against a format
> picture. tvision-rs ports it as [`PXPictureValidator`]; the recursive matching
> engine is taken **verbatim** from `tvalidat.cpp` into a transient `Picture`
> scanner (the scan cursors are per-call scratch, not validator state, because the
> `Validator` methods are `&self`/object-safe). Deviations: D2 (trait), D12
> (streaming dropped). The guide documents 1 field, 6 methods, and Table 19.41
> (picture format characters).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Pic` (field) | 512 | EQUIVALENT | OK | `PXPictureValidator.pic: String` | N/A | Guide: `Pic: PString` pointing to the format picture. Idiomatic: Pascal `PString` → owned `String`. C++ heap-allocs `pic = newStr(aPic)`; rust owns it. Private. The engine reads `pic.as_bytes()` (byte-level by design). |
| `Init` (constructor) | 512 | PORTED | OK | `PXPictureValidator::new(pic: impl Into<String>, auto_fill: bool)` | 3 | Guide/C++: copies `APic`, sets `voFill` bit if `AutoFill`, then runs `picture("", False)` as a syntax probe — if it ≠ `prEmpty`, `status = vsSyntax`. Rust: stores `pic`, `auto_fill`; runs `Picture::new(pic, []).run(false)` and sets `status_ok = (result == PicResult::Empty)`. The empty-input syntax-probe-at-construction is faithful (tests `pic_trailing_semicolon_is_syntax_error`, `pic_unbalanced_bracket_is_syntax_error`, `pic_well_formed_status_ok`). Rustdoc now covers what + how/when (syntax probe at construction; check is_status_ok after new). |
| `Load` (constructor) | 512 | NOT-PORTED | — | — | — | Stream constructor; `TStreamable` dropped (deviation D12). |
| `Done` (destructor) | 512 | NOT-PORTED | — | — | — | C++ frees the heap `pic`; rust's owned `String` drops automatically. No explicit destructor (RAII). |
| `Error` (method) | 513 | PORTED | OK | `PXPictureValidator::error(&mut Context)` | 3 | Guide: "Displays a message box indicating an error in the picture format, displaying the string pointed to by `Pic`." C++: `messageBox(mfError\|mfOKButton, errorMsg, pic)`. Rust: `"Error in picture format.\n {pic}"` via `ctx.request_message_box`. Matches (quotes the mask). Rustdoc now names the message text and that `validate` calls it automatically. |
| `IsValidInput` (method) | 513 | PORTED | OK | `PXPictureValidator::is_valid_input(&self, s: &mut String, suppress_fill) -> bool` | 3 | Guide: checks `S` against `Pic`, returns `True` if `Pic` is nil OR `Picture` ≠ `prError`; `SuppressFill` overrides `voFill` for this call; `var S` may be auto-filled/transformed. C++: `doFill = (voFill set) && !suppressFill; return (pic==0) \|\| (picture(s, doFill) != prError)`. Rust: `do_fill = self.auto_fill && !suppress_fill`; runs the engine, writes the (possibly grown/transformed) buffer back into `*s`, returns `r != PicResult::Error`. The `pic==0` guard is unnecessary (`pic` is always a String — documented). Mutation (autofill literals + uppercase) preserved (tests `pic_uppercase_autofill_mutates`, `pic_literal_colon_autofills`, `pic_literal_letter_normalizes_to_mask_case`). Matches. |
| `IsValid` (method) | 513 | PORTED | OK | `PXPictureValidator::is_valid(&self, s) -> bool` | 3 | Guide: returns `True` if `Pic` is nil OR `Picture` returns `prComplete` for `S` — i.e. no further input needed. C++: `picture(copy, False) == prComplete`. Rust: runs the engine on a copy with `auto_fill=false`, returns `result == PicResult::Complete` (no write-back). Matches (tests `pic_three_digits_is_valid`, `pic_optional_zip_plus_four`, `pic_comma_alternatives`, `pic_iteration_and_group`). Rustdoc now covers what (Complete means fully satisfied) + how/when (no mutation; for while-typing see is_valid_input). |
| `Picture` (method) | 513–514 | PORTED | OK | `Picture::run(&mut self, auto_fill) -> PicResult` (+ the whole `Picture` engine) | 3 | Guide: formats `Input` per `Pic`; `prError` on a picture error or unfittable data; `prComplete` if fully satisfied; `prIncomplete` if it fits but is partial. C++ `picture()` (syntaxCheck → empty→prEmpty → process → trailing-input→prError → autofill+reprocess → map prAmbiguous→prComplete / prIncompNoFill→prIncomplete). **The entire engine is ported verbatim** and traced function-for-function against `tvalidat.cpp`: `consume`, `to_group_end`, `skip_to_comma`, `calc_term`, `iteration` (counted vs greedy `*`, the `prEmpty→prIncomplete` and `prError→prAmbiguous` post-fixups, `index++` on greedy exit), `group` (`{}`/`[]`, `process(termCh-1)`, the `!isIncomplete → index=termCh` reset), `check_complete` (skip trailing optional `[`/`*` → prAmbiguous), `scan` (every Table-19.41 arm + literal/`;`-escape + the `prAmbiguous→prIncompNoFill` per-iter fixup + the trailing `prIncompNoFill→prAmbiguous` / else `prComplete`), `process` (comma-backtracking, farthest-incomplete tracking, the `incomp`/`incompJ` logic), `syntax_check`. Cursors are `i32`; `pic_at`/`input_at` synthesize a `0` NUL past the end to stay panic-free + byte-faithful. **No divergence found.** Byte-level (ASCII) by design, matching the C++ `char*` engine. |
| `Store` (method) | 514 | NOT-PORTED | — | — | — | Stream write of `Pic`; `TStreamable` dropped (deviation D12). |
| Table 19.41 — `#` (digit) | 513 | PORTED | OK | `scan` arm `b'#'` → `is_number` | 3 | Accept only a digit; else `prError`. C++ `case '#'`. Matches. |
| Table 19.41 — `?` (letter, case-insensitive) | 513 | PORTED | OK | `scan` arm `b'?'` → `is_letter` | 3 | Accept only a letter; else `prError`. Matches. |
| Table 19.41 — `&` (letter → uppercase) | 513 | PORTED | OK | `scan` arm `b'&'` → `is_letter` + `uppercase` | 3 | Letter, forced upper. `consume(uppercase(ch))`. Matches (test `pic_uppercase_autofill_mutates`). |
| Table 19.41 — `@` (any char) | 513 | PORTED | OK | `scan` arm `b'@'` → `consume(ch)` | 3 | Accept any character. Matches. |
| Table 19.41 — `!` (any → uppercase) | 513 | PORTED | OK | `scan` arm `b'!'` → `consume(uppercase(ch))` | 3 | Any char, forced upper. Matches (test `pic_bang_uppercases_single`). |
| Table 19.41 — `;` (literal escape) | 513 | PORTED | OK | `scan` default arm: `if pic==';' { index++ }` | 3 | "Take next character literally." Matches C++ literal-arm `;` handling. |
| Table 19.41 — `*` (repetition count) | 514 | PORTED | OK | `Picture::iteration` (`scan` arm `b'*'`) | 3 | `*[n]<group>`: `n` times, or greedy if 0. Full counted/greedy logic ported. Matches (test `pic_iteration_and_group`). |
| Table 19.41 — `[]` (option) | 514 | PORTED | OK | `Picture::group` (`scan` arm `b'['`) | 3 | Optional group; incomplete propagates, error→ambiguous. Matches (test `pic_optional_zip_plus_four`). |
| Table 19.41 — `{}` (grouping) | 514 | PORTED | OK | `Picture::group` (`scan` arm `b'{'`) | 3 | Required group. Matches (test `pic_iteration_and_group`). |
| Table 19.41 — `,` (alternatives) | 514 | PORTED | OK | `Picture::process` + `skip_to_comma` | 3 | Set of alternatives; first completing branch wins (with trailing-input → prError). Matches (test `pic_comma_alternatives`). |
| Table 19.41 — all others (literal) | 514 | PORTED | OK | `scan` default arm: case-insensitive literal match, mask byte consumed | 3 | "Taken literally." A typed space matches any literal; the buffer is normalized to the MASK's literal byte/case (not the typed char). Matches C++ default arm (test `pic_literal_letter_normalizes_to_mask_case`). |

> **Note on `IsStatusOk`/syntax:** the guide does not list a separate `syntax`
> method (it is C++ `syntaxCheck`, private, run inside `picture()`/`Init`). Rust
> exposes it as the free `syntax_check` + the `is_status_ok` override returning
> `status_ok`. Faithful: rejects empty mask, trailing `;`, or unbalanced `[]`/`{}`.

## Summary

- PORTED: 16   EQUIVALENT: 1   NOT-PORTED: 3   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable findings: The recursive Paradox-mask engine is ported **verbatim** and was traced function-for-function (`scan`/`process`/`iteration`/`group`/`check_complete`/`skip_to_comma`/`syntax_check`) against `tvalidat.cpp` — including every subtle post-fixup (`prAmbiguous↔prIncompNoFill`, `prEmpty→prIncomplete`, greedy `index++`, trailing-input→prError, autofill-then-reprocess) and the final `prAmbiguous→prComplete` / `prIncompNoFill→prIncomplete` public mapping. No validation-logic divergence found; the only deltas are idiomatic (`PString`→`String`, byte-level over `&[u8]`/`Vec<u8>`, a per-call transient scanner because methods are `&self`, the fixed 256-byte C++ buffer replaced by a growable `Vec`). All 12 Table-19.41 character classes are present and behaviour-tested with hand-traced golden vectors.
