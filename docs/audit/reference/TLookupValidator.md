# TLookupValidator  (guide pp. 474–475)

Rust module(s): `src/validate.rs`   |   magiblot: `include/tvision/validate.h` / `source/tvision/tvalidat.cpp`

> TLookupValidator is an **abstract** intermediate: its only own methods are
> `isValid` (which delegates to the virtual `lookup`) and `lookup` (which
> accepts everything by default). Concrete subclasses — `TStringLookupValidator`
> — override `lookup`. The C++ `isValid→lookup` indirection collapses in the
> port (deviation D2): each concrete lookup validator folds the lookup directly
> into `is_valid`.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `IsValid` (method) | 474 | EQUIVALENT | OK | `tv::LookupValidator` impl of `Validator` (accept-all default `is_valid`) | 3 | C++ `TLookupValidator::isValid` delegates to virtual `lookup(s)` — returning whatever `lookup` returns. Rust collapses the indirection: `LookupValidator` simply inherits the trait default `is_valid` that returns `true` (accept-all), which is identical in behaviour since `TLookupValidator::lookup` also returns `true` by default. The collapse is deliberate and commented. Struct rustdoc now explains when to use `LookupValidator` vs `StringLookupValidator` (the how/when gap from score 2). |
| `Lookup` (method) | 474 | EQUIVALENT | OK | absorbed into `tv::LookupValidator`'s accept-all `Validator` impl | N/A | C++ `TLookupValidator::lookup` is a virtual `Boolean` returning `True` (the default accept-all hook that subclasses override). Rust collapses `lookup`-as-a-separate-virtual into the direct `is_valid` override on each concrete type (D2). On `LookupValidator` itself the net result is identical accept-all behaviour. No separate `lookup` method exists or is needed; the mapping note in the struct doc covers this. |

## Summary

- PORTED: 0   EQUIVALENT: 2   NOT-PORTED: 0   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable finding: Both guide entries map to the same `LookupValidator` struct; the C++ `isValid→lookup` virtual indirection is correctly collapsed under D2, but the struct rustdoc only scores 2 — it explains *what* the collapse is but does not explain *when* a caller would use `LookupValidator` directly vs. `StringLookupValidator` (the "how/when" for the abstract-base role).
