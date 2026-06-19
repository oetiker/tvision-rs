# TMultiCheckBoxes  (guide pp. 486–488)

Rust module(s): `src/widgets/cluster.rs`   |   magiblot: `include/tvision/dialogs.h` / `source/tvision/tmulchkb.cpp`

> TMultiCheckBoxes is a concrete cluster where each item cycles through
> `SelRange` distinct states (not just on/off). `Value` packs multiple n-bit
> state fields: `Flags` (a `cfXXXX` constant) specifies how many bits each
> item occupies; `States` is a string of marker characters, one per state.
> The Rust port is a thin embed-and-delegate wrapper (`MultiCheckBoxes {
> cluster: Cluster }`) over the shared `Cluster` engine with
> `ClusterKind::MultiCheckBoxes { sel_range, flags, states }` (deviation D2).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Flags` (field) | 486 | PORTED | OK | `ClusterKind::MultiCheckBoxes { flags: u16, .. }` | 3 | Raised: variant field doc now explains lo/hi byte semantics with a concrete example (`0x0203` = 2-bit items, mask 0x03, stride 2) and capacity implications. |
| `SelRange` (field) | 486 | PORTED | OK | `ClusterKind::MultiCheckBoxes { sel_range: u8, .. }` | 3 | Raised: variant field doc now clarifies cycling behavior (0 → 1 → … → sel_range-1 → 0) and the relationship to `states` string length. |
| `States` (field) | 486 | PORTED | OK | `ClusterKind::MultiCheckBoxes { states: String, .. }` | 3 | Raised: variant field doc now states the indexing contract (char at index `s` = marker for state `s`), minimum length requirement, and out-of-range fallback. |
| `Init` (constructor) | 486 | PORTED | OK | `MultiCheckBoxes::new(bounds, strings, sel_range, flags, states)` | 3 | Raised: `MultiCheckBoxes::new` doc now describes each parameter, flags encoding, states contract, and starting state (value=0, every item in state 0). |
| `Load` (constructor) | 487 | NOT-PORTED | — | — | — | `TStreamable` dropped crate-wide. |
| `Done` (destructor) | 487 | NOT-PORTED | — | — | — | Rust `Drop` handles memory automatically; no explicit destructor. |
| `DataSize` (method) | 487 | EQUIVALENT | OK | cluster opt-out of D10 value protocol | N/A | No public Rust symbol — opt-out documented in module `//!` block. |
| `Draw` (method) | 487 | PORTED | OK | `Cluster::draw` (multi branch via `multi_mark` + `states` string) | N/A | No own `draw` on `MultiCheckBoxes` — fully delegated to `Cluster::draw`. No separate public symbol to score. |
| `GetData` (method) | 487 | EQUIVALENT | OK | cluster opt-out of D10 value protocol | N/A | No public Rust symbol — opt-out documented in module `//!` block. |
| `MultiMark` (method) | 487 | PORTED | OK | `Cluster::multi_mark(item: i32) -> usize` | N/A | Private engine method — not held to the public bar. Overflow guard (`checked_shl`) documented inline. |
| `Press` (method) | 487 | PORTED | OK | `Cluster::press` (`ClusterKind::MultiCheckBoxes` arm: cycles `0 → 1 → … → sel_range-1 → 0`) | N/A | Private engine method — not held to the public bar. |
| `SetData` (method) | 488 | EQUIVALENT | OK | cluster opt-out of D10 value protocol | N/A | No public Rust symbol — opt-out documented in module `//!` block. |
| `Store` (method) | 488 | NOT-PORTED | — | — | — | `TStreamable` dropped crate-wide. |
| `MultiCheckBoxes` (struct) | — | EQUIVALENT | OK | `pub struct MultiCheckBoxes` | 3 | Raised: struct-level doc now explains multi-state semantics, when to use, the `flags` encoding table, how to decode `cluster.value` after the dialog, and the note that `cfXXXX` constants are not ported (use `u16` literals). |

## Summary

- PORTED: 7   EQUIVALENT: 3   NOT-PORTED: 3   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Raised to 3: `MultiCheckBoxes` struct, `MultiCheckBoxes::new`, `ClusterKind::MultiCheckBoxes` variant fields (`flags`, `sel_range`, `states`). Private/no-own-symbol rows reclassified N/A.
- Notable: `cfXXXX` constants (`cfOneBit`, `cfTwoBits`, etc.) not ported — struct doc notes callers pass the `u16` literal directly.
