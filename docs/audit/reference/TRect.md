# TRect  (guide pp. 518–519)

Rust module(s): `src/view/geometry.rs` (re-exported as `tv::Rect`)   |   magiblot: `include/tvision/objects.h`

> TRect is a rectangle defined by two TPoint corners.  The 1992 guide documents
> 2 fields and 8 methods; magiblot's C++ header adds `operator==`, `operator!=`,
> `isEmpty`, two-point and coordinate constructors, and stream operators
> (ipstream/opstream).  The guide's `Assign` and `Copy` are Pascal-era helpers
> that have direct Rust structural equivalents.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `A` (field, TPoint) | 518 | PORTED | OK | `tv::Rect::a: Point` | 3 | Field now has its own doc comment: first cell of the rectangle (inclusive), explains a>b validity. |
| `B` (field, TPoint) | 518 | PORTED | OK | `tv::Rect::b: Point` | 3 | Field now has its own doc comment: one past last occupied column/row, width/height formula. |
| `Assign` (procedure, sets all four coordinates) | 518 | EQUIVALENT | OK | `tv::Rect::new(ax, ay, bx, by)` (const fn) | 3 | `new` doc now explains when to prefer it over `from_points` and restates the a/b convention. |
| `Contains` (function, Boolean) | 518 | PORTED | OK | `tv::Rect::contains(&self, p: Point) -> bool` | 3 | Method doc now states half-open semantics inline with rationale for character-cell hit-testing. |
| `Copy` (procedure, copy from R: TRect) | 518 | EQUIVALENT | OK | `#[derive(Clone, Copy)]` + struct assignment `r1 = r2` | N/A | Guide: `Copy(R: TRect)` sets all fields from R. Rust: `Rect` is `Copy`, so `r1 = r2` copies all fields. No named method needed. N/A: not a public method. |
| `Empty` (function, Boolean) | 518 | PORTED | OK | `tv::Rect::is_empty(&self) -> bool` | 3 | Doc now notes zero-area, zero-dimension, and inverted cases; links to `grow` for the deflate path. |
| `Equals` (function, Boolean) | 519 | EQUIVALENT | OK | `#[derive(PartialEq, Eq)]` (`r1 == r2`) | N/A | Guide: `Equals(R: TRect): Boolean`. C++ `operator==` in magiblot. Rust: derived `PartialEq`; `r1 == r2` works. N/A: derived, not a named public function. |
| `Grow` (procedure, ADX ADY: Integer) | 518 | PORTED | OK | `tv::Rect::grow(&mut self, dx: i32, dy: i32) -> &mut Self` | 3 | Doc now explains each-edge expansion formula, negative-deflate behaviour, and inverted-rect result. |
| `Intersect` (procedure, R: TRect) | 519 | PORTED | OK | `tv::Rect::intersect(&mut self, r: &Rect) -> &mut Self` | 3 | Doc now describes largest-fitting-rectangle semantics, non-overlapping → empty, and use-case (clip child to parent). |
| `Move` (procedure, ADX ADY: Integer) | 519 | PORTED | OK | `tv::Rect::r#move(&mut self, dx: i32, dy: i32) -> &mut Self` | 3 | Doc now covers how/when (reposition without resize, neg = left/up) and restates the raw-identifier call syntax. |
| `Union` (procedure, R: TRect) | 519 | PORTED | OK | `tv::Rect::r#union(&mut self, r: &Rect) -> &mut Self` | 3 | Doc now covers smallest-bounding-box semantics, use-case (dirty region), and raw-identifier call syntax. |
| `operator==` (C++ header) | — | EQUIVALENT | OK | `#[derive(PartialEq, Eq)]` | N/A | See `Equals` above. N/A. |
| `operator!=` (C++ header) | — | EQUIVALENT | OK | `#[derive(PartialEq)]` (provides `!=`) | N/A | Derived automatically. N/A. |
| `isEmpty` (C++ header, not in guide) | — | PORTED | OK | `tv::Rect::is_empty` | 3 | Covered under guide `Empty` entry above; score raised with that entry. |
| Two-arg constructor `TRect(TPoint, TPoint)` (C++ header) | — | EQUIVALENT | OK | `tv::Rect::from_points(p1: Point, p2: Point)` (const fn) | 3 | Doc now states when to prefer over `new`, maps p1→a / p2→b, and notes no sorting. |
| Default constructor `TRect()` → zeros (C++ header) | — | EQUIVALENT | OK | `#[derive(Default)]` → `Rect { a: (0,0), b: (0,0) }` | N/A | C++ `TRect()` zero-initializes. Rust: `Default::default()` yields the same. Verified by test `rect_constructors_equivalent`. N/A: derived. |
| stream `operator>>` / `operator<<` (C++ header) | — | NOT-PORTED | — | — | — | Borland TStreamable serialization layer. Known idiomatic mapping: `TStreamable`/streams → dropped (serde-if-revived). Intentional. |
| `Hash` / `Debug` (Rust extras) | — | NOT-PORTED | — | `#[derive(Debug, Hash)]` | — | Rust additions for ergonomics; not guide entries. Out-of-scope additions, not gaps. |

## Summary

- PORTED: 9   EQUIVALENT: 7   NOT-PORTED: 2   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable findings: All 10 previously-score-2 public symbols raised to score 3. Fields `a`/`b` now have individual doc comments. `contains` explains half-open hit-test semantics inline. `grow`/`is_empty` cover the negative-deflate and inverted-rect cases. `r#move`/`r#union` repeat the raw-identifier call syntax in the method doc itself. A runnable chaining doctest was added to the struct.
