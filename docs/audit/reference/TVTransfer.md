# TVTransfer type  (guide pp. 576–577)

Rust module(s): `src/data.rs`, `src/validate.rs`   |   magiblot: `include/tvision/validate.h` (line 35)

> `TVTransfer` is a three-value enum used as the `flag` parameter of
> `TValidator::Transfer(s, buffer, flag)`. The three values control whether
> the call returns the size of the transfer buffer (`vtDataSize`), writes the
> field value into the buffer (`vtGetData`), or reads from the buffer into
> the field (`vtSetData`). tvision-rs replaces this entire untyped
> getter/setter protocol with the typed D10 value protocol (`FieldValue` +
> `Validator::transfer_get` / `transfer_set`), so `TVTransfer` as a named
> type does not exist in the port.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TVTransfer` (enum type) | 576 | EQUIVALENT | OK | `src/data.rs::FieldValue` + `Validator::transfer_get` / `transfer_set` pair — D10 value protocol | 3 | C++: a bare `enum TVTransfer {vtDataSize, vtSetData, vtGetData}` used as the direction flag in `TValidator::Transfer(char* s, void* buffer, TVTransfer flag)`. The entire untyped buffer/flag protocol is replaced wholesale by D10: `transfer_get(&self, s: &str) -> Option<FieldValue>` extracts a typed value (the `vtGetData` direction); `transfer_set(&self, v: &FieldValue) -> Option<String>` formats a value back (the `vtSetData` direction); `vtDataSize` has no analog because the typed `FieldValue` enum carries its own size implicitly. Module-level docs in both `data.rs` and `validate.rs` document the D10 substitution clearly. `Validator::transfer_get/transfer_set` rustdoc raised to 3 (now explains gather/scatter walk context). |
| `vtDataSize` (enum variant) | 576 | NOT-PORTED | — | — | — | C++: passed to `Transfer` to query the byte size of the buffer the validator wants. Used by the dialog framework to allocate the right-sized untyped record. The D10 typed protocol makes buffer sizing unnecessary — `FieldValue` is a Rust enum whose size is statically known and the framework uses a `Vec<Option<FieldValue>>` rather than a raw byte block. No analog needed or present. |
| `vtSetData` (enum variant) | 576 | EQUIVALENT | OK | `tv::Validator::transfer_set(&self, v: &FieldValue) -> Option<String>` | 3 | C++: `Transfer(s, buf, vtSetData)` copies a typed value *from* the buffer *into* the field string. Rust: `transfer_set(v)` converts a `FieldValue` back to the field's text string (`Some(text)`) or `None` (not applicable / wrong type). Direction semantics match. Rustdoc now specifies called during **scatter** walk and instructs overriding alongside `transfer_get`. |
| `vtGetData` (enum variant) | 576 | EQUIVALENT | OK | `tv::Validator::transfer_get(&self, s: &str) -> Option<FieldValue>` | 3 | C++: `Transfer(s, buf, vtGetData)` copies the field's current value *into* the buffer. Rust: `transfer_get(s)` returns `Some(FieldValue)` when the validator has transfer enabled, `None` otherwise (the input line then keeps its plain text value). Direction semantics match. Rustdoc now specifies called during **gather** walk and instructs overriding alongside `transfer_set`. |

## Summary

- PORTED: 0   EQUIVALENT: 3   NOT-PORTED: 1   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable finding: The entire `TVTransfer` protocol is correctly replaced by the D10 typed `FieldValue` + `transfer_get`/`transfer_set` pair; `vtDataSize` has no analog (not needed, by design). The two public `transfer_*` methods score 2 — they lack a sentence explaining where in the gather/scatter walk they are called, which would help callers understand the protocol without reading the module doc.
