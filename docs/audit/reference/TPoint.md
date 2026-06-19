# TPoint  (guide p. 501)

Rust module(s): `src/view/geometry.rs` (re-exported as `tv::Point`)   |   magiblot: `include/tvision/objects.h`

> TPoint is a simple struct with two fields and six arithmetic/comparison
> operators.  The guide documents only the two fields; all operators are in the
> C++ header.  The Borland 1992 guide shows no methods — just `X` and `Y`.
> magiblot adds `+=`, `-=`, `+`, `-`, `==`, `!=` plus stream operators (ipstream/opstream).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `X` (field, Integer) | 501 | PORTED | OK | `tv::Point::x: i32` | 3 | Guide: screen column. Rust: `pub x: i32`. Uses `i32` rather than Pascal `Integer` (16-bit) — faithful to magiblot which uses `int`. Field doc now explains zero = leftmost column, negative = scrolled offscreen, and refers to `Point::new` for two-axis construction. |
| `Y` (field, Integer) | 501 | PORTED | OK | `tv::Point::y: i32` | 3 | Guide: screen row. Same i32 rationale as X. Field doc now explains zero = topmost row, negative = scrolled above visible area. |
| `operator+=` (C++ header) | — | EQUIVALENT | OK | `impl AddAssign for Point` | 3 | C++: `TPoint& operator+=(const TPoint&)`. Rust: `AddAssign` trait, `p += q`. Method doc now includes concrete usage example (`cursor += delta`). |
| `operator-=` (C++ header) | — | EQUIVALENT | OK | `impl SubAssign for Point` | 3 | C++: `TPoint& operator-=(const TPoint&)`. Rust: `SubAssign` trait, `p -= q`. Method doc now includes concrete usage example (`cursor -= delta`). |
| `operator+` (C++ header) | — | EQUIVALENT | OK | `impl Add for Point` | 3 | C++: `friend TPoint operator+(const TPoint&, const TPoint&)`. Rust: `Add` trait, returns new `Point`. Method doc now includes concrete usage example (`child_origin + scroll_offset`). |
| `operator-` (C++ header) | — | EQUIVALENT | OK | `impl Sub for Point` | 3 | C++: `friend TPoint operator-(const TPoint&, const TPoint&)`. Rust: `Sub` trait. Method doc now includes concrete usage example (`mouse_pos - view_origin` → view-local coordinates). |
| `operator==` (C++ header) | — | EQUIVALENT | OK | `#[derive(PartialEq, Eq)]` | N/A | C++: `friend int operator==(const TPoint&, const TPoint&)`. Rust: derived `PartialEq` / `Eq`. `p == q` works identically. N/A: derived, not a named public symbol to score. |
| `operator!=` (C++ header) | — | EQUIVALENT | OK | `#[derive(PartialEq)]` (provides `!=`) | N/A | C++ has an explicit `operator!=`. Rust's derived `PartialEq` provides `!=` automatically. N/A for same reason. |
| stream `operator>>` / `operator<<` (C++ header) | — | NOT-PORTED | — | — | — | ipstream/opstream are the Borland TStreamable serialisation layer. Per known idiomatic mapping: `TStreamable`/streams → dropped (serde-if-revived). No Rust analog exists; intentional. |
| `new` / default constructor (C++ TRect has one; TPoint relies on POD zero-init) | — | EQUIVALENT | OK | `tv::Point::new(x, y)` (const fn) + `#[derive(Default)]` | 3 | C++ TPoint is POD; zero-init is implicit. Rust: `Point::new(x, y)` for explicit construction; `Default::default()` → `(0, 0)`. Doc now notes `const fn` usability, links `Default` as the zero-point idiom, and includes a doctest showing `ORIGIN == Point::default()`. |
| `Hash` / `Copy` / `Clone` / `Debug` (Rust extras) | — | NOT-PORTED | — | `#[derive(Clone, Copy, Debug, Hash)]` | — | C++ TPoint has none of these. Rust derives them as quality-of-life additions consistent with the project's Rust-idiomatic approach. These are additions, not missing items. NOT-PORTED is N/A here — these are Rust extras, not guide entries. Classified as out-of-scope additions. |

## Summary

- PORTED: 2   EQUIVALENT: 7   NOT-PORTED: 2   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable findings: All previously below-bar symbols (`x`, `y`, `Point::new`, and the four arithmetic operator impls) raised to score 3. `Point` struct-level doc now includes a usage example and explains the signed-coordinate rationale. `operator==`/`!=` remain N/A (derived). Stream operators remain intentionally NOT-PORTED.
