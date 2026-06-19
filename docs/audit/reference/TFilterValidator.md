# TFilterValidator  (guide pp. 441–443)

Rust module(s): src/validate.rs   |   magiblot: include/tvision/validate.h / source/tvision/tvalidat.cpp

> A filter validator gates input against a set of allowed characters: every
> character of the field must be a member of the set. tvision-rs ports it as
> [`FilterValidator`] (deviation D2; streaming dropped, D12). The guide documents
> 1 field and 5 methods.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `ValidChars` (field) | 442 | EQUIVALENT | OK | `FilterValidator.valid_chars: String` | N/A | Guide: `ValidChars: TCharSet` — the set of allowed chars (e.g. `['0'..'9']`). Idiomatic: a `TCharSet` (256-bit set) becomes an owned `String`; membership is `valid_chars.contains(c)` per Unicode `char`. C++ tests per byte via `strspn`; rust tests per `char` — identical for the ASCII charsets these carry (documented in the heritage note). Private field. |
| `Init` (constructor) | 442 | PORTED | OK | `FilterValidator::new(valid_chars: impl Into<String>)` | 3 | Guide: sets `ValidChars` to `AValidChars`. Rust `new` stores the set. Matches. Rustdoc now covers what + how/when (pass a string literal of accepted chars; attach to InputLine). |
| `Load` (constructor) | 442 | NOT-PORTED | — | — | — | Stream constructor; `TStreamable` dropped (deviation D12). |
| `Error` (method) | 442 | PORTED | OK | `FilterValidator::error(&mut Context)` | 3 | Guide: "Displays a message box indicating the text string contains an invalid character." C++: `messageBox(mfError\|mfOKButton, errorMsg)`. Rust pops "Invalid character in input" via `ctx.request_message_box` (Error kind, OK-only). Matches behaviour; the literal string is tvision-rs's own (C++ `errorMsg` is a resource). Rustdoc now names what is popped and that `validate` calls it automatically. |
| `IsValid` (method) | 442 | PORTED | OK | `FilterValidator::is_valid(&self, s) -> bool` | 3 | Guide: `True` iff all chars of `S` are in `ValidChars`. C++: `strspn(s, validChars) == strlen(s)` — note this makes the **empty string valid** (`strspn("")==0==strlen("")`). Rust: `s.chars().all(\|c\| valid_chars.contains(c))` — also true for `""` (vacuous all). Empty-input behaviour matches exactly (test `filter_accepts_empty_string`). Rustdoc now covers what + how/when (empty string passes; for while-typing see is_valid_input). |
| `IsValidInput` (method) | 442 | PORTED | OK | `FilterValidator::is_valid_input(&self, s: &mut String, _suppress_fill) -> bool` | 3 | Guide: same char-set check while typing; `SuppressFill` ignored (a filter never fills); does not mutate. C++ `isValidInput` body is identical to `isValid` (`strspn==strlen`). Rust delegates to `is_valid`, ignores `suppress_fill`, never mutates `s`. Matches exactly. Rustdoc now clarifies never-mutates + suppress_fill ignored. |
| `Store` (method) | 442 | NOT-PORTED | — | — | — | Stream write of `ValidChars`; `TStreamable` dropped (deviation D12). |

## Summary

- PORTED: 4   EQUIVALENT: 1   NOT-PORTED: 2   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable findings: No divergences. The validation logic is a faithful port — including the subtle "empty string is valid" semantics of `strspn(s)==strlen(s)`, which `chars().all(..)` preserves (vacuous truth) and which a dedicated test pins. The byte-vs-`char` membership difference is documented and benign for ASCII charsets. `is_valid_input` correctly ignores `suppress_fill` (filters never auto-fill).
