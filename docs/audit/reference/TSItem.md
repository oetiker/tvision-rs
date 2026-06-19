# TSItem type  (guide pp. 530–531)

Rust module(s): src/widgets/cluster.rs   |   magiblot: include/tvision/dialogs.h / source/tvision/tcluster.cpp

> `TSItem` is a singly-linked list node used to pass a list of label strings to
> cluster constructors (`TCheckBoxes.Init`, `TRadioButtons.Init`, etc.) and list
> widgets. The companion utility function `NewSItem` allocates a node on the heap
> and chains it: `NewSItem('Option A', NewSItem('Option B', nil))`.
> In tvision-rs this entire pattern is replaced by `Vec<String>` passed directly
> to constructors — an idiomatic Rust substitution (known mapping: linked list → Vec).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TSItem` record type (`Value: PString; Next: PSItem`) | 530 | EQUIVALENT | OK | `Vec<String>` passed to cluster/list constructors | N/A | Singly-linked heap list of Pascal `PString`s. Rust replaces the entire pattern with a plain `Vec<String>` argument (e.g. `Cluster::new(bounds, strings, kind)`, `CheckBoxes::new(bounds, strings)`). No `TSItem` struct exists — the linked-list node shape is never exposed. Known idiomatic mapping: `TCollection` family / linked list → `Vec`. Private implementation detail; N/A for doc score. |
| `Value` field (`PString`) | 530 | EQUIVALENT | OK | element of `Vec<String>` | N/A | The string payload of each node maps to a `String` element. Private. |
| `Next` field (`PSItem`) | 530 | EQUIVALENT | OK | implicit Vec ordering | N/A | The "next node" pointer is replaced by Vec indexing. No explicit field. Private. |
| `NewSItem` utility function (`function NewSItem(S: String; Next: PSItem): PSItem`) | 531 | EQUIVALENT | OK | `Vec<String>` literal / `vec![...]` macro at call sites | N/A | `NewSItem` was the ergonomic list-builder: `NewSItem("A", NewSItem("B", nil))`. In tvision-rs callers write `vec!["A".into(), "B".into()]` or collect a `Vec<String>`. No `new_sitem` function exists — not needed with `Vec`. Known idiomatic mapping. |

## Summary

- PORTED: 0   EQUIVALENT: 4   NOT-PORTED: 0   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable findings: Clean idiomatic substitution — the singly-linked `TSItem`/`NewSItem` builder pattern is entirely replaced by `Vec<String>`, eliminating manual heap allocation and null-termination. No public symbols to document; no gaps.
