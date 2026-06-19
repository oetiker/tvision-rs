# TCluster  (guide pp. 395–399)

Rust module(s): `src/widgets/cluster.rs`   |   magiblot: `include/tvision/dialogs.h` / `source/tvision/tcluster.cpp`

> TCluster is the abstract base for all cluster-style controls. The Rust port
> collapses the abstract base + concrete subclasses into a single engine
> (`Cluster`) branching on a closed `ClusterKind` enum, with thin
> embed-and-delegate wrappers for the named types (deviations D1, D2).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `EnableMask` (field) | 396 | PORTED | OK | `Cluster::enable_mask: u32` | 3 | Raised: doc now covers what (bitmask, 0xFFFF_FFFF default), when to use `set_button_state` vs. mutating directly, and the 32-item cap. |
| `Sel` (field, read-only) | 396 | PORTED | OK | `Cluster::sel: i32` | 3 | Raised: doc covers what (visual cursor index), how CheckBoxes/MultiCheckBoxes vs. RadioButtons differ, and the RadioButtons gotcha (set both sel+value when restoring state). |
| `Strings` (field, read-only) | 396 | EQUIVALENT | OK | `Cluster::strings: Vec<String>` | 3 | Raised: doc covers hotkey marker syntax, column-major fill order, and the ~-stripping for column-width calculation. |
| `Value` (field, read-only) | 396 | PORTED | OK | `Cluster::value: u32` | 3 | Raised: doc covers per-kind interpretation (bitmask / index / packed), how to read each kind, and when to write directly. |
| `Init` (constructor) | 396 | EQUIVALENT | OK | `Cluster::new(bounds, strings, kind)` + `CheckBoxes::new` / `RadioButtons::new` / `MultiCheckBoxes::new` | 3 | Raised: `Cluster::new` now states the initial state (value=0, sel=0, all enabled, cursor at (2,0)) and when to prefer the named constructors over calling it directly. |
| `Load` (constructor) | 397 | NOT-PORTED | — | — | — | `TStreamable` / stream load/store dropped crate-wide (deviation: serde-if-revived). |
| `Done` (destructor) | 397 | NOT-PORTED | — | — | — | Rust `Drop` is automatic; no explicit destructor needed. |
| `ButtonState` (method) | 397 | PORTED | OK | `Cluster::button_state(item: i32) -> bool` | N/A | Private method — not held to the public bar. Useful internal comment present. |
| `DataSize` (method) | 397 | EQUIVALENT | OK | Cluster does not impl `View::value` / `data_size`; dialog gather/scatter skips clusters | N/A | No public Rust symbol — the opt-out is documented in the module `//!` block. EQUIVALENT with no direct mapping. |
| `DrawBox` (method) | 397 | EQUIVALENT | OK | `ClusterKind::icon()` + `Cluster::draw` (inlined) | 1 | No public symbol; inlined into `Cluster::draw`. Functionally equivalent; doc score 1 (private impl detail). |
| `DrawMultiBox` (method) | 397 | EQUIVALENT | OK | `Cluster::draw` (multi branch via `multi_mark`) | 1 | No public symbol; inlined. Doc score 1 (private impl detail). |
| `GetData` (method) | 398 | EQUIVALENT | OK | cluster opt-out of D10 value protocol (see `data.rs` module doc) | N/A | No public Rust symbol — opt-out documented in module `//!` block. |
| `GetHelpCtx` (method) | 398 | EQUIVALENT | OK | `HelpCtx` offset-by-sel not modeled | N/A | No public Rust symbol — deviation documented in module `//!` block ("not modeled"). |
| `GetPalette` (method) | 398 | EQUIVALENT | OK | `ctx.style(Role::Cluster*)` in `Cluster::draw` | N/A | No public Rust symbol — palette resolved via `DrawCtx::style` in `Cluster::draw`; Role items are in `src/theme.rs` (theme pass). |
| `HandleEvent` (method) | 398 | PORTED | OK | `Cluster::handle_event` (impl `View::handle_event`) | 3 | Guide: handles mouse (click/drag) and keyboard (arrows, Space, Alt-hotkey, plain-letter). Rust: full implementation with `MouseDown`/`MouseMove`/`MouseUp` hold-tracking capture, four arrow navigators, accelerator scan, focused-Space. One deliberate deviation (deferred focus vs. synchronous `focus()`) is commented inline (`tcluster.cpp:283`). |
| `Mark` (method) | 398 | PORTED | OK | `Cluster::mark(item: i32) -> bool` | N/A | Private method — not held to the public bar. |
| `MovedTo` (method) | 398 | PORTED | OK | `Cluster::moved_to(item: i32)` | N/A | Private method — not held to the public bar. |
| `MultiMark` (method) | 398 | PORTED | OK | `Cluster::multi_mark(item: i32) -> usize` | N/A | Private method — not held to the public bar. |
| `Press` (method) | 398 | PORTED | OK | `Cluster::press(item: i32)` | N/A | Private method — not held to the public bar. |
| `SetButtonState` (method) | 399 | PORTED | OK | `Cluster::set_button_state(a_mask: u32, enable: bool)` | 3 | Raised: doc now covers what (enable/disable by bitmask), how selectability is recomputed, and includes a `rust,ignore` usage example. |
| `SetData` (method) | 399 | EQUIVALENT | OK | cluster opt-out of D10 value protocol | N/A | No public Rust symbol — opt-out documented in module `//!` block. |
| `SetState` (method) | 399 | PORTED | OK | `Cluster` does not override `View::set_state`; whole-tree redraw covers it | N/A | No public Rust symbol (no override exists); the module `//!` block explains the whole-tree redraw replaces per-state DrawView calls. |
| `Store` (method) | 399 | NOT-PORTED | — | — | — | `TStreamable` dropped crate-wide. |
| `CCluster` palette (4 entries) | 399 | EQUIVALENT | OK | `Role::ClusterNormal`, `ClusterNormalShortcut`, `ClusterSelected`, `ClusterSelectedShortcut`, `ClusterDisabled` | 3 | Role items documented in `src/theme.rs` (theme pass): each variant names the widget, color, and chain. |
| `ClusterKind` (enum) | — | EQUIVALENT | OK | `pub enum ClusterKind` | 3 | Raised: enum-level doc now explains when to use each variant and points to the named constructors as the preferred entry point. |

## Summary

- PORTED: 11   EQUIVALENT: 10   NOT-PORTED: 3   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Raised to 3: `enable_mask`, `sel`, `strings`, `value`, `Cluster::new`, `set_button_state`, `ClusterKind`. Private/no-symbol rows reclassified N/A.
- `CCluster` palette `Role` items raised to 3 in the theme.rs Role pass (documented in `src/theme.rs`).
