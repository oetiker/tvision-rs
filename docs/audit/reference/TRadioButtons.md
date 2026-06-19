# TRadioButtons  (guide pp. 514–516)

Rust module(s): `src/widgets/cluster.rs`   |   magiblot: `include/tvision/dialogs.h` / `source/tvision/tradiobu.cpp`

> TRadioButtons is a concrete cluster where exactly one button is selected at
> any time. `Value` is the index of the selected (pressed) button; selecting a
> new button automatically deselects the previous one. The Rust port is a thin
> embed-and-delegate wrapper (`RadioButtons { cluster: Cluster }`) over the
> shared `Cluster` engine with `ClusterKind::RadioButtons` (deviation D2).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Draw` (method) | 515 | PORTED | OK | `Cluster::draw` (`ClusterKind::RadioButtons` arm: icon `" ( ) "`, marker `'•'`) | N/A | No own `draw` on `RadioButtons` — fully delegated to `Cluster::draw`. No separate public symbol to score. |
| `Mark` (method) | 515 | PORTED | OK | `Cluster::mark` (`ClusterKind::RadioButtons` arm: `item == value as i32`) | N/A | Private engine method — not held to the public bar. |
| `MovedTo` (method) | 515 | PORTED | OK | `Cluster::moved_to` (`ClusterKind::RadioButtons` arm: `value = item as u32`) | N/A | Private engine method — not held to the public bar. Semantics (arrow-key navigation also updates value) documented in `Cluster::sel` field doc. |
| `Press` (method) | 515 | PORTED | OK | `Cluster::press` (`ClusterKind::RadioButtons` arm: `value = item as u32`) | N/A | Private engine method — not held to the public bar. |
| `SetData` (method) | 515 | EQUIVALENT | OK | cluster opt-out of D10 value protocol (module doc) | N/A | No public Rust symbol — opt-out documented in module `//!` block. The `sel = value` initialization note is covered by `Cluster::sel` field doc (set both when restoring state). |
| `CCluster` palette | 515 | EQUIVALENT | OK | `Role::ClusterNormal/ClusterSelected/ClusterNormalShortcut/ClusterSelectedShortcut/ClusterDisabled` | 3 | Role items documented in `src/theme.rs` (theme pass): each variant names the widget, color, and chain. |
| `Init` / `Load` / `Done` (constructors) | 514 | EQUIVALENT / NOT-PORTED | — | `RadioButtons::new(bounds, strings)` / NOT-PORTED | 3 | Raised: `RadioButtons::new` doc now states starting state (item 0 selected, value=0), hotkey marker format, and the radiobutton-specific gotcha (set both `cluster.value` and `cluster.sel` to pre-select). |
| `RadioButtons` (struct) | — | EQUIVALENT | OK | `pub struct RadioButtons` | 3 | Raised: struct-level doc now explains the mutual-exclusion semantics, when to use RadioButtons vs. CheckBoxes, how to pre-select, and includes a `rust,ignore` usage example. |

## Summary

- PORTED: 4   EQUIVALENT: 2   NOT-PORTED: 0   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Raised to 3: `RadioButtons` struct, `RadioButtons::new`. Private/no-own-symbol rows reclassified N/A.
- `CCluster` palette `Role` items raised to 3 in the theme.rs Role pass (documented in `src/theme.rs`).
