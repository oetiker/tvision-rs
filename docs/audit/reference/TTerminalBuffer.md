# TTerminalBuffer  (guide pp. 555–556)

Rust module(s): `src/widgets/terminal.rs`   |   magiblot: `include/tvision/textview.h` / `source/tvision/textview.cpp`

> **Heritage note:** `TTerminalBuffer` is a Pascal record type that the 1992
> Borland TV 2.0 guide documents separately as the circular-buffer storage
> used by `TTerminal`. It is **not a class in the magiblot C++ port** — the
> C++ equivalent is the set of three fields `buffer`, `queFront`, `queBack`
> (plus `bufSize`) held directly on `TTerminal`. In Rust the same fields live
> on `Terminal`. There is no separate `TTerminalBuffer` type in either the
> C++ source or the Rust port. Each entry below is classified against its
> structural equivalent in `Terminal`.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TTerminalBuffer` (type itself) | 555 | NOT-PORTED | — | — | — | Pascal record type; dissolved into fields on `TTerminal` in C++ and then on `Terminal` in Rust. No standalone type exists or is needed. |
| `buffer` (field) | 555 | PORTED | OK | `Terminal.buffer: Vec<u8>` | N/A | C++: `char *buffer` — heap-allocated array of `bufSize` bytes, freed in destructor. Rust: `Vec<u8>` pre-allocated to `buf_size` in `new()`, dropped automatically. Semantically identical; Rust ownership replaces raw `new[]`/`delete[]`. Private. |
| `bufSize` (field) | 555 | PORTED | OK | `Terminal.buf_size: usize` | N/A | C++: `ushort bufSize`, capped at `min(32000, aBufSize)`. Rust: `usize`, capped via `.clamp(1, 32000)`. One-slot sentinel invariant preserved: `buf_size - 1` is max usable capacity. Private. |
| `queFront` (field) | 556 | PORTED | OK | `Terminal.que_front: usize` | N/A | Write head. C++: `ushort queFront`. Rust: `usize`. Same semantics: points to the next empty slot. Private. |
| `queBack` (field) | 556 | PORTED | OK | `Terminal.que_back: usize` | N/A | Read tail. C++: `ushort queBack`. Rust: `usize`. Points to the oldest data byte. Empty iff `que_front == que_back`. Private. |

## Summary

- PORTED: 4   EQUIVALENT: 0   NOT-PORTED: 1   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0 (all private / N/A)   |   → concept: 0
- Notable finding: `TTerminalBuffer` as a distinct type was a Pascal artifact; the C++ port dissolved it into `TTerminal` fields and the Rust port faithfully mirrors that. All four underlying storage fields are present and correct. No gaps.
