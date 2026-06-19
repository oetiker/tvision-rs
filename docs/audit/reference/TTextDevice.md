# TTextDevice  (guide pp. 556–557)

Rust module(s): `src/widgets/terminal.rs`   |   magiblot: `include/tvision/textview.h` / `source/tvision/textview.cpp`

> **Architecture note:** In C++, `TTextDevice` is an abstract class that
> inherits both `TScroller` and `streambuf`. It provides the `do_sputn`
> pure-virtual entry point (overridden by `TTerminal`) and the `overflow`
> bridge method that feeds single characters through `do_sputn` to make the
> class usable as a C++ `ostream` target. The `xsputn` non-Borland override
> routes bulk writes through `do_sputn` for performance.
>
> The Rust port replaces all stream plumbing with the `TextDevice` trait
> (deviations D11, D12). The trait has a single method `write_bytes` — the
> direct equivalent of `do_sputn`. There is no `overflow` / `xsputn` because
> Rust has no `streambuf` / `ostream` hierarchy. The `otstream` C++ wrapper
> class (which let `TTerminal` be used as an `ostream`) is also not ported
> (D12: stream persistence/plumbing dropped).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Init` (constructor) | 556 | PORTED | OK | `TextDevice` is a trait; construction is `Terminal::new` | N/A | C++ `TTextDevice(bounds, aHScrollBar, aVScrollBar)`: delegates straight to `TScroller`. Rust: `Terminal::new` constructs the embedded `Scroller` with the same three parameters. There is no `TextDevice::new` because the trait has no data. |
| `do_sputn` (method) | 556 | EQUIVALENT | OK | `TextDevice::write_bytes(&mut self, data: &[u8], ctx: &mut Context) -> usize` | 3 | C++: `virtual int do_sputn(const char *s, int count) = 0` — pure virtual. Rust: single required method on the `TextDevice` trait. Same role: the bulk-write entry point that concrete types override. The signature adds `ctx` (needed for deferred scrollbar updates). Return type changes from `int` (bytes accepted) to `usize` (always `data.len()` for `Terminal`). The trait doc explains the stream-plumbing replacement (D11/D12) and the `ctx` addition. Score 3. |
| `overflow` (method) | 557 | NOT-PORTED | — | — | — | C++ `streambuf::overflow(int c)`: called by the `ostream` layer when the internal buffer is full or for single-character puts; bridges to `do_sputn` for one byte. Rust has no `streambuf`/`ostream` hierarchy (D11/D12). Single-byte writes go via `write_bytes(&[c], ctx)` directly. No Rust analog exists or is needed. |
| `xsputn` (method) | 557 | NOT-PORTED | — | — | — | C++ non-Borland override: `std::streamsize xsputn(const char *s, std::streamsize count)` — bypasses the default `streambuf::xsputn` (which would call `overflow` per-byte) and routes bulk writes directly through `do_sputn`. Performance optimization for the `ostream` layer. No analog in Rust: the `ostream` layer does not exist (D12); `write_bytes` already handles bulk writes natively. |
| `GetPalette` (method) | 557 | EQUIVALENT | OK | Inherited from `Scroller` via `#[delegate(to = scroller)]`; `Role::ScrollerNormal` used in `Terminal::draw` | 2 | C++ `TTextDevice` inherits `TScroller::getPalette()` which returns `cpScroller = "\x06\x07"` (2 entries). `TTerminal::draw` calls `mapColor(1)` → `cpScroller[1]` → window slot 6 → `cpAppColor[0x0D] = 0x1E`. Rust: no `get_palette` override on `Terminal` or on the `TextDevice` trait; `Terminal::draw` calls `ctx.style(Role::ScrollerNormal)` directly. Known mapping: class Palette → `tv::Theme` (D7). The module doc notes "the color map becomes a `Role`". Score 2: what the role is, not the chain. → concept: palette chain guide. **Deferred: `→ concept` row — left as-is.** |
| `otstream` (companion class) | 557 | NOT-PORTED | — | — | — | C++ `otstream : public ostream`: a thin wrapper that accepts a `TTerminal*` and exposes it as an `ostream` so callers can use `<<` stream operators. Dropped (D12: stream persistence/plumbing removed). Rust callers use `write_bytes` directly or implement `std::io::Write` on a wrapper if needed — that would be a deliberate extension, not a port gap. |
| `CTextDevice` palette (inherited) | 557 | EQUIVALENT | OK | `Role::ScrollerNormal` + `Role::ScrollerSelected` via `Theme` (inherited from `Scroller`) | 3 | `TTextDevice` has no own palette; it inherits `TScroller`'s `CScroller`. Both `Role::ScrollerNormal` (yellow on blue, `0x1E`, chain) and `Role::ScrollerSelected` (blue on lightgray, `0x71`, chain) documented in `src/theme.rs` (theme pass). |

## Summary

- PORTED: 1   EQUIVALENT: 3   NOT-PORTED: 3   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 1 (→ concept row)   |   → concept: 1
- Notable finding: All three NOT-PORTED entries (`overflow`, `xsputn`, `otstream`) are C++ `streambuf`/`ostream` plumbing with no equivalent in Rust's I/O model; their absence is correct and follows documented deviations D11/D12. No gaps. `CTextDevice` palette raised to 3 in the theme.rs Role pass. `GetPalette` remains at 2 as a `→ concept` row (no public Rust symbol to annotate; the role lookup is inlined in `Terminal::draw`).
