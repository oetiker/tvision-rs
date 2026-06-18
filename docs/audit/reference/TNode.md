# TNode  (guide p. 488)

Rust module(s): src/widgets/outline.rs   |   magiblot: include/tvision/outline.h / source/tvision/toutline.cpp

> `TNode` is a plain record (struct in Pascal terms) with four fields and two
> constructors. The guide documents it on p. 488 as the node type that holds
> the tree structure for `TOutlineViewer` and `TOutline`.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `next` (field) | 488 | PORTED | OK | `tv::Node::next: Option<Box<Node>>` | 2 | C++ raw pointer; Rust uses `Option<Box<Node>>` — owned next-sibling. The `Box` drop recursively frees the sibling chain; behavior is identical. Doc says what it is, not when to use builder vs. direct assign. |
| `text` (field) | 488 | PORTED | OK | `tv::Node::text: String` | 2 | C++ `const char*` (heap-allocated via `newStr`); Rust `String` owns the text. Same semantics; the destructor difference is automatic. |
| `childList` (field) | 488 | PORTED | OK | `tv::Node::child_list: Option<Box<Node>>` | 2 | C++ raw pointer to first child; Rust `Option<Box<Node>>`. Same singly-linked structure, ownership automatic. |
| `expanded` (field) | 488 | PORTED | OK | `tv::Node::expanded: bool` | 2 | C++ `Boolean`; Rust `bool`. New nodes default to `true` (expanded) in both. Doc says what it is; "how it interacts with `adjust`/`isExpanded`" is not mentioned. |
| Constructor `TNode(aText)` | 488 | PORTED | OK | `tv::Node::new(text: impl Into<String>) -> Node` | 2 | Single-arg constructor: null `next`/`childList`, `expanded = True`. Rust default: same. |
| Constructor `TNode(aText, aChildren, aNext, initialState)` | 488 | PORTED | OK | `tv::Node::with_next` / `tv::Node::with_children` / `tv::Node::with_expanded` (builder chain) | 2 | C++ 4-arg constructor sets all fields. Rust uses a builder pattern — `Node::new(text).with_children(…).with_next(…).with_expanded(…)`. Known idiomatic mapping. Doc explains each builder method individually (what), but not the builder-chain idiom vs. the C++ constructor. |
| Destructor `~TNode` | 488 | EQUIVALENT | OK | Automatic `Box<Node>` / `Drop` | N/A | C++ destructor frees `text`; the recursive delete of `next`/`childList` is done by `TOutlineViewer::disposeNode`. Rust: `Box` drop is recursive — the entire owned subtree is freed when the root box drops. No explicit destructor; equivalent by construction. Private/internal. |

## Summary

- PORTED: 5   EQUIVALENT: 1   NOT-PORTED: 0   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 6   |   → concept: 0
- Notable findings: No gaps or correctness issues. All four fields and both constructors are ported faithfully. The main doc gap is that none of the public items score 3 — the builder-chain pattern and its relationship to the C++ 4-arg constructor, plus the ownership/drop semantics, would each benefit from a short "how/when" sentence.
