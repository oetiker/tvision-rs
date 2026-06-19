# TPicResult type  (guide pp. 501–502)

Rust module(s): src/validate.rs   |   magiblot: include/tvision/validate.h / source/tvision/tvalidat.cpp

> `TPicResult` is the result type returned by `TPXPictureValidator.Picture`.
> tvision-rs ports it as the **private** `enum PicResult` (`#[derive(Clone, Copy,
> PartialEq, Eq, Debug)]`) used internally by the picture engine. It is NOT part
> of the public API: the public `is_valid`/`is_valid_input` methods return `bool`
> (the C++ also folds `Picture`'s result down to a `Boolean` at the call sites),
> so `PicResult` is an implementation detail — a deliberate, reasonable
> narrowing (the seven-way result only matters inside the recursive matcher). All
> seven C++ variants are present.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TPicResult` (type) | 501 | EQUIVALENT | OK | `enum PicResult` (private) | N/A | Guide: `TPicResult = (prComplete, prIncomplete, prEmpty, prError, prSyntax, prAmbiguous, prIncompNoFill)`; the result type of `Picture`. Idiomatic: Pascal enum → Rust enum, same seven variants in the same order. Made **private** (not `pub`) because the public surface returns `bool` — the engine-internal seven-way result is never exposed. Module-level doc comment on the enum (`what each variant means`). |
| `prComplete` (variant) | 501 | PORTED | OK | `PicResult::Complete` | N/A | Input fully satisfies the picture. Drives `is_valid` (`== Complete`). Matches. |
| `prIncomplete` (variant) | 501 | PORTED | OK | `PicResult::Incomplete` | N/A | Fits the picture but partial. `is_incomplete` helper covers `Incomplete \| IncompNoFill`. Matches. |
| `prEmpty` (variant) | 501 | PORTED | OK | `PicResult::Empty` | N/A | Empty input. Returned early by `run` when input is empty; also the syntax-probe "well-formed" signal in `new`. Matches. |
| `prError` (variant) | 501 | PORTED | OK | `PicResult::Error` | N/A | Picture error / data cannot fit. `is_valid_input` returns `false` only on this. Matches. |
| `prSyntax` (variant) | 501 | PORTED | OK | `PicResult::Syntax` | N/A | Malformed mask (failed `syntax_check`). Drives `status_ok` (a non-`Empty` probe ⇒ not ok). Matches. |
| `prAmbiguous` (variant) | 501 | PORTED | OK | `PicResult::Ambiguous` | N/A | An internal "ambiguously complete" state; `is_complete` covers `Complete \| Ambiguous`. `run` maps `Ambiguous → Complete` on the public boundary. Matches. |
| `prIncompNoFill` (variant) | 501 | PORTED | OK | `PicResult::IncompNoFill` | N/A | Internal "incomplete, no fill" state; `run` maps `IncompNoFill → Incomplete` on the public boundary. Matches. |

## Summary

- PORTED: 7   EQUIVALENT: 1   NOT-PORTED: 0   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable findings: All seven `TPicResult` variants are ported in the same order with identical semantics, including the two engine-internal states (`Ambiguous`/`IncompNoFill`) that the public boundary maps down to `Complete`/`Incomplete`. The enum is deliberately **private** (the public API returns `bool`, matching how C++ folds `Picture`'s result to a `Boolean` at every call site) — a reasonable narrowing, so `EQUIVALENT`/OK rather than a gap. No public symbols, so no doc-score concerns.
