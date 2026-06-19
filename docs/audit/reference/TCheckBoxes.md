# TCheckBoxes  (guide pp. 393–395)

Rust module(s): `src/widgets/cluster.rs`   |   magiblot: `include/tvision/dialogs.h` / `source/tvision/tcheckbo.cpp`

> TCheckBoxes is a concrete cluster for independent on/off toggles. `Value` is
> a bitmask: bit `i` set ⇔ item `i` is checked. The Rust port is a thin
> embed-and-delegate wrapper (`CheckBoxes { cluster: Cluster }`) over the shared
> `Cluster` engine with `ClusterKind::CheckBoxes` (deviation D2).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Draw` (method) | 394 | PORTED | OK | `Cluster::draw` (dispatched via `#[delegate]`) | N/A | No own `draw` on `CheckBoxes` — fully delegated to `Cluster::draw`. No separate public symbol to score. The struct-level doc now describes the rendering (` [ ] ` box, `X` marker). |
| `Mark` (method) | 394 | PORTED | OK | `Cluster::mark` (`ClusterKind::CheckBoxes` arm: `value & (1 << item) != 0`) | N/A | Private engine method — not held to the public bar. Behavior (bits 0–31, `item >= 32 → false`) described via `Cluster::value` field doc. |
| `Press` (method) | 394 | PORTED | OK | `Cluster::press` (`ClusterKind::CheckBoxes` arm: `value ^= 1 << item`) | N/A | Private engine method — not held to the public bar. XOR toggle with overflow guard for `item >= 32`. |
| `CCluster` palette | 395 | EQUIVALENT | OK | `Role::ClusterNormal/ClusterSelected/ClusterNormalShortcut/ClusterSelectedShortcut/ClusterDisabled` via `DrawCtx` | 3 | Role items documented in `src/theme.rs` (theme pass): each variant names the widget, color, and chain (e.g. `cpCluster[1]=0x10 → cpGrayDialog[16]=0x2F → cpAppColor[47]=0x30`). |
| `Init` (constructor) | 393 | EQUIVALENT | OK | `CheckBoxes::new(bounds: Rect, strings: Vec<String>)` | 3 | Raised: doc now states starting state (value=0, sel=0, all enabled, cursor at item 0), label format (`~X~` hotkey marker), and refers callers to `cluster.value` for result inspection. |
| `Load` / `Done` / stream methods | 393 | NOT-PORTED | — | — | — | `TStreamable` dropped crate-wide. Note: guide states "TCheckBoxes does not override TCluster constructors, destructor, or event handler" — consistent with Rust delegation. |
| `CheckBoxes` (struct) | — | EQUIVALENT | OK | `pub struct CheckBoxes` | 3 | Raised: struct-level doc now explains the bitmask semantics, when to use `CheckBoxes` vs. `RadioButtons`, and includes a `rust,ignore` usage example with result reading. |

## Summary

- PORTED: 3   EQUIVALENT: 2   NOT-PORTED: 1   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Raised to 3: `CheckBoxes` struct, `CheckBoxes::new`. Private/no-own-symbol rows reclassified N/A.
- `CCluster` palette `Role` items raised to 3 in the theme.rs Role pass (documented in `src/theme.rs`).
