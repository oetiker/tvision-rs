# TValidator  (guide pp. 557–560)

Rust module(s): src/validate.rs   |   magiblot: include/tvision/validate.h / source/tvision/tvalidat.cpp

> `TValidator` is the abstract data-validation base. tvision-rs ports it as the
> object-safe [`Validator`] **trait** (`Option<Box<dyn Validator>>` storage in an
> input line); the subclass hierarchy becomes concrete impls (deviation D2). The
> guide documents 2 fields, 7 methods, the `vsXXXX` status constants, and the
> `voXXXX` option flags (the last two enumerated from validate.h, the guide only
> cross-references them).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Options` (field) | 557 | EQUIVALENT | OK | per-validator bools (`RangeValidator.transfer_enabled`, `PXPictureValidator.auto_fill`) | N/A | Guide: `Options: Word` bitmapped (`voFill`/`voTransfer`), cleared by `Init`. Idiomatic mapping: flag word → struct-of-bools, split per concrete validator. No single shared `options` word (the trait has no fields); each option lives where it is used. Private fields. |
| `Status` (field) | 557 | EQUIVALENT | OK | `Validator::is_status_ok() -> bool` (overridden by `PXPictureValidator.status_ok`) | 3 | Guide: `Status: Word`; `vsOk` (0) = constructed correctly, anything else = error. Idiomatic: the `Word` status collapses to a bool query (only `vsOk`/`vsSyntax` exist). `is_status_ok` returns whether status is OK. Matches. |
| `Init` (constructor) | 558 | EQUIVALENT | OK | trait has no ctor; each impl has `new` | N/A | Guide: sets `Options`/`Status` to zero. The trait base has no state; each concrete `new` sets its own defaults (transfer off, status ok). Faithful by mapping. |
| `Load` (constructor) | 558 | NOT-PORTED | — | — | — | Stream constructor; `TStreamable` machinery dropped (deviation D12). |
| `Error` (method) | 558 | PORTED | OK | `Validator::error(&mut Context)` | 3 | Guide: abstract, called by `Valid` on invalid input; base does nothing; descendants pop a message box. Rust default is a no-op; concrete impls call `ctx.request_message_box` (async-modal-from-a-view seam). Threads `&mut Context` (not in C++ signature) so a `&self` validator can request its box — documented deviation. Matches. |
| `IsValidInput` (method) | 558 | PORTED | OK | `Validator::is_valid_input(&self, s: &mut String, suppress_fill: bool) -> bool` | 3 | Guide: called after each keystroke; default `True`; `var S` (may mutate — uppercase/insert literals, must not delete); `SuppressFill` honoured only by `TPXPictureValidator`. Rust: `&mut String` (the `var S`), default `true`, only `PXPictureValidator` reads `suppress_fill`. Matches exactly. |
| `IsValid` (method) | 559 | PORTED | OK | `Validator::is_valid(&self, s: &str) -> bool` | 3 | Guide: validates a *completed* line; default `True`. Rust default `true`; concrete impls override. Matches. Rustdoc now covers what + how/when (override this for your rule; use `validate` when you need the error box). |
| `Store` (method) | 559 | NOT-PORTED | — | — | — | Writes `Options` to a stream; `TStreamable` dropped (deviation D12). |
| `Transfer` (method) | 559 | EQUIVALENT | OK | `Validator::transfer_get(&self, s) -> Option<FieldValue>` + `transfer_set(&self, &FieldValue) -> Option<String>` | 3 | (already 3; gather/scatter how/when now on both methods) | Guide: one `Transfer(var S, Buffer, Flag: TVTransfer)` switching on `vtDataSize`/`vtGetData`/`vtSetData`; default returns 0 (= "not handled"); only `TRangeValidator` overrides. Idiomatic: the untyped buffer + 3-way flag becomes the D10 typed pair (`transfer_get`/`transfer_set` over `FieldValue`); `None` = the C++ "return 0" not-handled sentinel; `vtDataSize` (just a size query) has no analog — the typed value carries its own size. Matches semantically. |
| `Valid` (method) | 560 | PORTED | OK | `Validator::validate(&self, s, &mut Context) -> bool` | 3 | Guide: `Valid` returns `True` if `IsValid(S)`, else calls `Error` and returns `False`. Rust `validate` is exactly this provided method (`if is_valid {true} else {error(); false}`). Name `validate` vs `Valid` (house style); threads `&mut Context` for `error`. Matches C++ `TValidator::validate` verbatim. Rustdoc now covers what + how/when (call at commit point; override is_valid/error, not this). |
| `vsOk` (constant) | 557 (decl in validate.h) | EQUIVALENT | OK | (implicit) `is_status_ok() == true` | N/A | Status enum collapses to a bool; `vsOk` = the `true` branch. No named const (only two states exist). |
| `vsSyntax` (constant) | 557 (validate.h) | EQUIVALENT | OK | (implicit) `is_status_ok() == false` for a malformed `PXPictureValidator` mask | N/A | The not-ok branch; set when the picture mask fails its syntax probe. No named const. |
| `voFill` (option flag) | validate.h | EQUIVALENT | OK | `PXPictureValidator.auto_fill: bool` | N/A | flag word bit → a bool on the one validator that uses it. |
| `voTransfer` (option flag) | validate.h | EQUIVALENT | OK | `RangeValidator.transfer_enabled: bool` (`set_transfer`) | N/A | flag word bit → a bool on the range validator. |
| `voReserved` (option flag) | validate.h | NOT-PORTED | — | — | — | Reserved-bits mask (`0x00FC`); no behavior, nothing to port. |
| `TVTransfer` (enum) | 559 (validate.h) | NOT-PORTED | — | — | — | `vtDataSize`/`vtSetData`/`vtGetData` flag for the single untyped `Transfer`; subsumed by the typed `transfer_get`/`transfer_set` split (D10). No standalone enum needed. |

## Summary

- PORTED: 4   EQUIVALENT: 8   NOT-PORTED: 4   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable findings: No gaps or divergences. The whole base maps cleanly: trait + concrete impls (D2), typed-transfer pair (D10), `&mut Context`-threaded `error` (async-modal seam), and streaming dropped (D12) — all documented deviations. `validate`/`Valid` is a verbatim port of the C++ provided method. The `Word` status and `options` bitfield collapse idiomatically (bool query + per-validator bools).
