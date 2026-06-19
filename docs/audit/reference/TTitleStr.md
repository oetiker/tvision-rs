# TTitleStr type  (guide p. 557)

Rust module(s): `src/window/window.rs` (field `Window::title`)   |   magiblot: `include/tvision/views.h`

> `TTitleStr` is a Pascal type alias (`TTitleStr = string[80]`) — a fixed-length
> 80-byte Pascal string. Its sole documented role is as the type of the
> `TWindow.Title` field. There are no methods; the guide entry is a single
> declaration + one-liner description.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TTitleStr` (type alias, `= string[80]`) | 557 | EQUIVALENT | OK | `Option<String>` (field `Window::title: Option<String>`) + `Window::title() -> Option<&str>` | N/A | Pascal `string[80]` is a fixed-capacity heap-or-stack Pascal string; `nil`/empty string = no title. Rust uses `Option<String>` (unbounded, heap-allocated) on the `Window` struct, with `None` for the no-title case — the capacity bound is irrelevant on modern hardware and the `None`-vs-empty distinction is cleaner than Pascal's empty-string convention. The type alias itself is not ported (Rust does not need a named alias for a built-in type). Known idiomatic mapping: Pascal `string[N]` type alias → Rust `String`/`&str`. `N/A` on doc score because the type alias is not a public symbol — only the field accessor is. |

## Summary

- PORTED: 0   EQUIVALENT: 1   NOT-PORTED: 0   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable finding: `TTitleStr` is a trivial Pascal-era type alias with no semantic content beyond "up to 80 chars, nil = no title." The Rust `Option<String>` on `Window` is a complete idiomatic equivalent; the capacity bound is irrelevant and the `None` convention is cleaner than an empty string. No gaps or suspect items.
