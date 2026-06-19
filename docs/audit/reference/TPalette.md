# TPalette  (guide pp. 498–499)

Rust module(s): `src/theme.rs`   |   magiblot: `include/tvision/views.h` (typedef)

> `TPalette` is a one-line type alias with no fields or methods. The guide documents only
> its declaration, its function, and its cross-reference. The idiomatic mapping
> from `TPalette` (a Pascal `String`) → `tv::Theme` (a role-keyed style table) is
> pre-declared in the audit README and should be classified EQUIVALENT, not MISSING.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TPalette = String` (type declaration) | 498 | EQUIVALENT | OK | `tv::Theme` + `tv::Role` | 3 | Guide: `TPalette` is a Pascal `String` — a length-prefixed byte array. Each widget returns a `TPalette` from `GetPalette`; the framework resolves colors by indexing that string then chasing the owner chain. Rust collapses the entire palette chain into one `Theme` keyed by a semantic `Role` enum (deviation D7). The `Theme` module doc explicitly calls out the deviation ("collapses the original palette chain") and every `Role` variant carries a rustdoc comment. Heritage section present and complete. |
| Function ("a string type used to declare Turbo Vision palettes") | 498 | EQUIVALENT | OK | `tv::Role` (semantic key) + `tv::Theme::style(role)` (lookup) | 3 | Guide describes the indirect lookup: `GetPalette` returns the string, callers index into it, then the owner chain resolves final attributes. Rust: `ctx.theme.style(Role::XxxYyy)` is the one-call replacement for the whole chain. `Theme::style` is documented (what + how). |
| `GetPalette` methods cross-reference | 498 | EQUIVALENT | OK | `tv::Theme::style` (no `GetPalette` method on views) | 3 | Guide says "see also: GetPalette methods." Each widget in the C++ source overrides `GetPalette` to return a `CXxx` palette string. In Rust, widgets do not have a `GetPalette` method; they call `ctx.theme.style(role)` directly. After the theme.rs Role pass, every `Role` variant carries full palette-chain derivation, color, and widget context in its rustdoc, making the D7 mapping completely traceable per-role. |

## Summary

- PORTED: 0   EQUIVALENT: 3   NOT-PORTED: 0   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable findings: After the theme.rs Role pass, every `Role` variant carries full palette-chain derivation, color, and widget context — the D7 mapping is now completely traceable per-role. The `GetPalette methods` cross-reference row raised to 3. No gaps remaining.
