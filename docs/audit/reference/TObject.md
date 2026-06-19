# TObject  (guide pp. 488–489)

Rust module(s): none — no TObject root type exists in tvision-rs   |   magiblot: `include/tvision/tv.h` / no dedicated header; TObject is the Turbo Pascal OOP root class

> TObject is the universal base class of all Turbo Vision objects in Pascal/C++.
> Rust has no single root object class; ownership + `Drop` + Rust's trait system
> replace its three responsibilities: heap allocation (`Init`), virtual
> destruction (`Done`), and `Free` (self-dispose).  There are NO fields.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Init` (constructor) | 488 | EQUIVALENT | OK | Rust struct construction (e.g. `Frame::new(...)`, `Button::new(...)`) | N/A | Pascal `Init` zeros heap memory and acts as the base constructor. Rust constructors (inherent `new` / builder fns) initialize fields directly; the language guarantees no uninitialized reads. Zero-initialization of fields is explicit via `Default` (e.g. `#[derive(Default)]`) or per-field. No single base-class `Init` is needed or exists. EQUIVALENT: same guarantee, idiomatic shape. |
| `Done` (virtual destructor) | 489 | EQUIVALENT | OK | Rust `Drop` trait (`fn drop(&mut self)`) + ownership-based RAII | N/A | Pascal `Done` is the virtual destructor; descendants override it to free resources. Rust's ownership system calls `drop` automatically when a value goes out of scope; complex teardown overrides `impl Drop`. Same invariant: every live object is eventually cleaned up, order determined by ownership tree. `Drop` is not a method users call directly — matching C++ `Done` semantics exactly. |
| `Free` (procedure) | 489 | EQUIVALENT | OK | Rust ownership / `drop(value)` / `Box<T>` deallocation | N/A | Pascal `Free` calls `Done` then disposes the heap object; it is a safe nil-pointer-aware self-dispose. In Rust, dropping the owning `Box<T>` (or letting a value go out of scope) calls `drop` and frees memory atomically. `std::mem::drop(x)` provides an explicit early-drop. No separate `Free` method exists or is needed; ownership makes it structurally impossible to `Free` twice (no double-free). |

## Summary

- PORTED: 0   EQUIVALENT: 3   NOT-PORTED: 0   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable findings: TObject has no direct Rust counterpart by design — its three responsibilities (`Init`, `Done`, `Free`) map cleanly onto Rust's struct constructors, the `Drop` trait, and ownership-based deallocation respectively. All three are EQUIVALENT/OK. No rustdoc score applies (no single public symbol represents TObject). This is the expected and correct outcome for a language-level design difference, not a gap.
