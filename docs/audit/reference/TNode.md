# TNode  (guide p. 488)

Rust module(s): src/widgets/outline.rs   |   magiblot: include/tvision/outline.h / source/tvision/toutline.cpp

> `TNode` is a plain record (struct in Pascal terms) with four fields and two
> constructors. The guide documents it on p. 488 as the node type that holds
> the tree structure for `TOutlineViewer` and `TOutline`.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `next` (field) | 488 | PORTED | OK | `tv::Node::next: Option<Box<Node>>` | 3 | Raised: rustdoc now explains that `next` is the next sibling, when to use `with_next` vs. direct assignment, and that the `Box` drop frees the sibling chain automatically. |
| `text` (field) | 488 | PORTED | OK | `tv::Node::text: String` | 3 | Raised: rustdoc explains that `text` is displayed verbatim and that reassigning it requires calling `ov_update` to refresh column widths. |
| `childList` (field) | 488 | PORTED | OK | `tv::Node::child_list: Option<Box<Node>>` | 3 | Raised: rustdoc explains the singly-linked structure, when to use `with_children`, and that the drop frees the whole subtree. |
| `expanded` (field) | 488 | PORTED | OK | `tv::Node::expanded: bool` | 3 | Raised: rustdoc explains default-true, how the user changes it interactively (- key / graph click) or programmatically via `with_expanded`, and the relationship to `is_expanded`/`adjust`. |
| Constructor `TNode(aText)` | 488 | PORTED | OK | `tv::Node::new(text: impl Into<String>) -> Node` | 3 | Raised: rustdoc says it creates a leaf node and directs readers to the builder chain before boxing and passing to `Outline::new`. |
| Constructor `TNode(aText, aChildren, aNext, initialState)` | 488 | PORTED | OK | `tv::Node::with_next` / `tv::Node::with_children` / `tv::Node::with_expanded` (builder chain) | 3 | Raised: each builder method now has a "how/when" sentence plus a doctest on `with_next` showing the sibling-list idiom. |
| Destructor `~TNode` | 488 | EQUIVALENT | OK | Automatic `Box<Node>` / `Drop` | N/A | Private/internal. No public symbol. |

## Summary

- PORTED: 6   EQUIVALENT: 1   NOT-PORTED: 0   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable findings: All six public items raised to score 3. Destructor row is N/A (no public symbol). Two doctests added (`Node` struct example, `with_next` sibling-list example); both pass `cargo test --doc`.
