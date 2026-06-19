# TRangeValidator  (guide pp. 516–518)

Rust module(s): src/validate.rs   |   magiblot: include/tvision/validate.h / source/tvision/tvalidat.cpp

> A range validator gates input through a sign-selected digit char-set filter,
> then on the final check parses the text and requires it within `[Min, Max]`.
> It is a `TFilterValidator` subclass; tvision-rs ports it as [`RangeValidator`]
> with an **embedded** `FilterValidator` (deviation D2, embed-and-delegate), the
> typed-transfer pair (D10), and streaming dropped (D12). The guide documents 2
> fields and 6 methods.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Max` (field) | 516 | EQUIVALENT | OK | `RangeValidator.max: i32` | N/A | Guide: `Max: Longint` (32-bit) — highest valid value. C++ uses `int32_t`. Rust `i32` — faithful to magiblot's `int32_t` (and the project's "coords/ints are `i32`" rule). Private. |
| `Min` (field) | 516 | EQUIVALENT | OK | `RangeValidator.min: i32` | N/A | Guide: `Min: Longint` — lowest valid value. C++ `int32_t` → rust `i32`. Private. Also selects the embedded filter's charset (sign of `min`). |
| `Init` (constructor) | 516 | PORTED | OK | `RangeValidator::new(min: i32, max: i32)` | 3 | Guide/C++: calls `TFilterValidator::Init` with digits `'0'..'9'` plus `'+'`/`'-'`; sets `Min`/`Max`. C++ picks `validUnsignedChars` if `aMin >= 0` else `validSignedChars`. Rust: `min >= 0` → `"+0123456789"`, else `"+-0123456789"`. The sign-of-min charset selection matches the C++ exactly (test `range_charset_selected_by_sign_of_min`). Transfer OFF by default (C++ `voTransfer` unset). Matches. Rustdoc now covers sign-of-min charset + set_transfer how/when. |
| `Load` (constructor) | 516 | NOT-PORTED | — | — | — | Stream constructor; `TStreamable` dropped (deviation D12). |
| `Error` (method) | 516 | PORTED | OK | `RangeValidator::error(&mut Context)` | 3 | Guide: "Displays a message box indicating that the entered value did not fall in the specified range." C++: `messageBox(mfError\|mfOKButton, errorMsg, min, max)` (range interpolated). Rust: `"Value not in the range {min} to {max}"` via `ctx.request_message_box` (Error, OK-only). Matches; the value-naming behaviour is preserved. Rustdoc now names the message text and notes `validate` calls it automatically. |
| `IsValid` (method) | 517 | PORTED | OK | `RangeValidator::is_valid(&self, s) -> bool` | 3 | Guide: three conditions — valid integer, `>= Min`, `<= Max`. C++: `TFilterValidator::isValid(s)` (charset gate) `&&` `sscanf(s,"%ld",&value)==1` `&&` `value in [min,max]`. Rust: `filter.is_valid(s) && parse_long(s).is_some_and(\|v\| v >= min && v <= max)`. Order (charset gate first, then parse+range) and the three conditions match. **Parse nuance (documented, OK):** rust's `str::parse::<i32>()` is stricter than `sscanf("%ld")` — it rejects trailing junk (`"12+3"`) and a lone `"+"`/`"-"`, whereas a `%ld` leading-run scan truncate-accepts. The charset filter already restricts the field to `[+-0-9]`, so clean input is identical; the divergence is only pathological mid-string sign/junk and is a commented, deliberate stricter simplification (`parse_long` doc-comment) → **not** suspect. Tests `range_is_valid_rejects_sign_only_string` pin the lone-sign case. Rustdoc now covers what (three conditions, charset gate first) + how/when. |
| `Store` (method) | 517 | NOT-PORTED | — | — | — | Stream write of `Min`/`Max`; `TStreamable` dropped (deviation D12). |
| `Transfer` (method) | 517 | EQUIVALENT | OK | `transfer_get(&self, s) -> Option<FieldValue>` + `transfer_set(&self, &FieldValue) -> Option<String>` | 3 | Guide: incorporates `DataSize`/`GetData`/`SetData`; uses a `Longint` data record (not a string); returns size of a `Longint` when `voTransfer` set, else 0. C++: switches on `TVTransfer` flag, `sscanf`/`sprintf` to/from `*(long*)buffer`. Idiomatic (D10): `vtGetData` → `transfer_get` (text → `FieldValue::Int`); `vtSetData` → `transfer_set` (`Int` → text via `to_string`, C++ `sprintf "%ld"`); `vtDataSize` → no analog (typed value self-sizes). Gated on `transfer_enabled` exactly like C++'s `voTransfer` check; disabled → `None` (the C++ "return 0"). Round-trip and disabled-default tests pass. Matches semantically. |

> **Note on `IsValidInput`:** the guide's TRangeValidator section does NOT list
> `IsValidInput` (it inherits `TFilterValidator::isValidInput` unchanged — a
> charset-only gate with no range check while typing). Rust faithfully delegates
> `is_valid_input` straight to the embedded filter, so a partial out-of-range
> number is accepted as input and the range is enforced only at `is_valid`
> (tests `range_is_valid_input_is_charset_only_not_range_checked`). This inherited
> behaviour is correct and matches C++.

## Summary

- PORTED: 3   EQUIVALENT: 3   NOT-PORTED: 2   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable findings: Validation logic is a faithful port. The one behavioural difference — `parse::<i32>()` rejecting `sscanf`-style trailing junk / lone signs — is a deliberate, commented stricter simplification (benign because the charset filter pre-restricts the field), so it is OK not SUSPECT. The sign-of-min charset selection, charset-gate-then-range order, value-naming error box, and the typed-transfer mapping (D10) all match the C++. Inherited `isValidInput` (charset-only, no range while typing) is correctly preserved.
