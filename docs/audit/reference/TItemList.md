# TItemList type  (guide p. 464)

Rust module(s): N/A   |   magiblot: `include/tvision/objects.h` (TCollection internal)

> **Audit scope clarification:** The task prompt names "TItemList" as the
> string-list type passed to clusters. However, the guide at p. 464 defines
> `TItemList` as a low-level internal type used by `TCollection`, **not** the
> type passed to cluster constructors. Clusters receive a linked list of
> `TSItem` nodes (built with `NewSItem`) — a completely different type.
>
> This file audits both:
> 1. `TItemList` as actually defined at p. 464 (internal `TCollection` array).
> 2. `TSItem` / `NewSItem` — the actual string-list mechanism passed to
>    cluster constructors (the likely intended subject of the prompt).

---

## TItemList (guide p. 464 — TCollection internal array)

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TItemList` type declaration | 464 | EQUIVALENT | OK | `Vec<Box<dyn Any>>` / `Vec<T>` inside `TCollection` (not a named public type) | N/A | Guide: `TItemList = array[0..MaxCollectionSize-1] of Pointer` — a raw pointer array used internally by `TCollection` for its heap-allocated items block. Rust: the `TCollection` family maps to idiomatic `Vec`; there is no raw `TItemList` type. Known idiomatic mapping: `TCollection` family → `Vec`/slices. Not a public symbol; N/A for rustdoc. |

---

## TSItem / NewSItem (the actual cluster string-list — the intended topic)

The cluster constructor `TCluster.Init(var Bounds; AStrings: PSItem)` takes a
linked list of `TSItem` records assembled with the `NewSItem` global function
(see guide p. 360 — `NewSItem function`). This is the "string-list type passed
to clusters."

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TSItem` type (`value: PString; next: PSItem`) | (360) | EQUIVALENT | OK | `Vec<String>` passed directly to `Cluster::new` / `CheckBoxes::new` etc. | N/A | C++: singly-linked list of heap-allocated strings, built right-to-left with `NewSItem`. Rust: the entire linked list is replaced by `Vec<String>` passed directly to constructors. Known idiomatic mapping: `TCollection` family / linked list → `Vec`. Not a named public type. |
| `NewSItem(Str, Next)` global function | 360 | EQUIVALENT | OK | `Vec<String>` literal or `vec![…]` | N/A | C++: allocates one node and links it. Rust: no analog needed — `Vec` literals replace the nested `NewSItem(…, NewSItem(…, nil))` construction pattern. |

---

## Summary

- PORTED: 0   EQUIVALENT: 3   NOT-PORTED: 0   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable finding: `TItemList` at p. 464 is a `TCollection`-internal raw pointer array, not the cluster string-list. The cluster string-list type is `TSItem`/`NewSItem` (p. 360). Both are fully replaced by `Vec<String>` in an idiomatic, clean mapping. No gaps.
