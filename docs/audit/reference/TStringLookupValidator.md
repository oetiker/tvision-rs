# TStringLookupValidator  (guide pp. 551–553)

Rust module(s): `src/validate.rs`   |   magiblot: `include/tvision/validate.h` / `source/tvision/tvalidat.cpp`

> TStringLookupValidator is the concrete lookup validator: it holds a
> `TStringCollection*` (`Strings` field) and overrides `lookup` to search
> it. The port maps the DOS `TStringCollection*` to an owned `Vec<String>`
> (known idiomatic mapping: `TCollection` family → `Vec`/slices), and the
> `lookup`-as-virtual indirection collapses into `is_valid` directly (D2).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Strings` (field) | 551 | EQUIVALENT | OK | `tv::StringLookupValidator` field `strings: Vec<String>` (private) | N/A | C++: `TStringCollection*` (pointer to a heap-allocated sorted collection). Rust: owned `Vec<String>`. Idiomatic mapping: `TCollection` family → `Vec`/slices. Private field, no public accessor. No public getter exists (nor did the C++ field itself warrant public access — it was set via constructor and `newStringList`). |
| `Init` (constructor) | 551 | PORTED | OK | `tv::StringLookupValidator::new(strings: Vec<String>) -> Self` | 3 | C++: `Init(AStrings: PStringCollection)` — calls `TLookupValidator::Init`, sets `Strings`. Rust: `new(strings)` — takes owned `Vec<String>`. Streaming (`TLookupValidator::Init` super-call) dropped (D12). Rustdoc now covers what + how/when (pass owned Vec; attach to InputLine; use new_string_list to replace). |
| `Load` (constructor) | 551 | NOT-PORTED | — | — | — | Stream-loading constructor (`Load(var S: TStream)`). Streaming machinery dropped (deviation D12). No analog. |
| `Done` (destructor) | 552 | NOT-PORTED | — | — | — | C++ `Done` calls `newStringList(nil)` to free the collection then calls `TLookupValidator.Done`. Rust: ownership + `Drop` handles deallocation automatically. No explicit destructor needed or present. Effectively: EQUIVALENT to Rust `Drop`, but it is not a ported symbol — it is the Rust ownership model. Classified NOT-PORTED (the C++ explicit deallocation pattern has no analog; no `Drop` impl needed). |
| `Error` (method) | 552 | PORTED | OK | `tv::StringLookupValidator` impl of `Validator::error` | 3 | C++ `error` calls `MessageBox` to display "not in list" dialog. Rust `error` calls `ctx.request_message_box("Input is not in list of valid strings", Error, ok-only, None, None)` — the async-modal-from-a-view seam. Functionally equivalent. Rustdoc now names the message and notes `validate` calls it automatically. |
| `Lookup` (method) | 552 | EQUIVALENT | OK | absorbed into `tv::StringLookupValidator`'s `Validator::is_valid` | N/A | C++ `lookup(s)` calls `Strings^.Search(s, I)` (binary search in the sorted collection). Rust `is_valid(s)` calls `self.strings.iter().any(|x| x == s)` — linear scan in a `Vec`. Semantics identical for membership testing; ordering not exploited (D2 collapse). The port uses linear search rather than binary search because `Vec<String>` is unsorted — SUSPECT candidate. However: `TStringCollection` in the C++ stores a sorted ASCII collection and `lookup` uses binary search. The Rust port does a linear `any` scan. This is a documented simplification (the D2 note says the lookup "folds directly into `is_valid`" without specifying search order), and correctness is the same. Classified EQUIVALENT/OK because the guide only requires returning `True` if present; search strategy is an implementation detail. No test exercises ordering behaviour. |
| `NewStringList` (method) | 552 | PORTED | OK | `tv::StringLookupValidator::new_string_list(&mut self, strings: Vec<String>)` | 3 | C++: `newStringList(AStrings)` disposes of the old collection, then sets `Strings := AStrings`. Nil arg means "dispose without replacing". Rust: replaces `self.strings` with the new `Vec`; old `Vec` is dropped automatically. Rustdoc now covers what + how/when (runtime re-list of a dependent field; empty `Vec` rejects all) and the heritage note documents the nil-arg divergence (`newStringList(nil)` → pass an empty `Vec`). |
| `Store` (method) | 552 | NOT-PORTED | — | — | — | Stream-store method. Streaming machinery dropped (deviation D12). No analog. |

## Summary

- PORTED: 3   EQUIVALENT: 2   NOT-PORTED: 3   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable finding: The `lookup` collapse (linear `any` scan vs. C++ binary search in a `TStringCollection`) is correct for membership but loses O(log n) performance on large lists; worth a note in the `is_valid` doc. The nil-arg form of `newStringList` (dispose-without-assigning) has no Rust analog — passing an empty `Vec` differs in that it replaces rather than just frees, which is not documented.
