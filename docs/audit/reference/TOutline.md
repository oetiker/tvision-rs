# TOutline  (guide pp. 489–491)

Rust module(s): src/widgets/outline.rs   |   magiblot: include/tvision/outline.h / source/tvision/toutline.cpp

> `TOutline` is the concrete outline viewer over an owned `TNode` tree. It
> derives from `TOutlineViewer` and provides concrete implementations of all
> the abstract navigation virtuals. The guide documents it on pp. 489–491
> (overview) and p. 492 (field `root`). In Rust it is `tv::Outline`, a struct
> that implements `OutlineViewer` and `View`.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `root` (field) | 491 | PORTED | OK | `tv::Outline::root: Option<Box<Node>>` | 3 | Raised: doc now explains `None` = empty tree, how siblings/children chain, Box drop behaviour, and how to swap the tree at runtime (assign + call `ov_update`). |
| `Init` (constructor) | 491 | PORTED | OK | `tv::Outline::new(bounds, h, v, root) -> Outline` | 3 | Raised: doc now explains what each parameter does, that `ov_update` is mandatory post-insert (Context unavailable at construction), and what happens if it is skipped. |
| `adjust` (method) | 491 | PORTED | OK | `tv::Outline`'s `OutlineViewer::adjust(pos: i32, expand: bool)` | 3 | Raised: impl-level doc now explains the DFS-position keying and why it differs from the C++ node-pointer signature — the shared traversal code is position-keyed throughout. |
| `getChild` (method) | 491 | PORTED | OK | `tv::Outline`'s `OutlineViewer::get_child<'a>(&'a self, node: &'a Node, i: i32) -> Option<&'a Node>` | 3 | Raised: impl doc notes the 0-based index walk and `None` when out of range. |
| `getNext` (method) | 491 | PORTED | OK | `tv::Outline`'s `OutlineViewer::get_next<'a>(&'a self, node: &'a Node) -> Option<&'a Node>` | 3 | Raised: impl doc says it follows `Node::next`. |
| `getNumChildren` (method) | 491 | PORTED | OK | `tv::Outline`'s `OutlineViewer::get_num_children(&self, node: &Node) -> i32` | 3 | Raised: impl doc describes the sibling-chain walk. |
| `getRoot` (method) | 491 | PORTED | OK | `tv::Outline`'s `OutlineViewer::get_root(&self) -> Option<&Node>` | 3 | Raised: impl doc says it returns `self.root.as_deref()`. |
| `getText` (method) | 491 | PORTED | OK | `tv::Outline`'s `OutlineViewer::get_text<'a>(&'a self, node: &'a Node) -> &'a str` | 3 | Raised: impl doc says it returns `&node.text`. |
| `hasChildren` (method) | 491 | PORTED | OK | `tv::Outline`'s `OutlineViewer::has_children(&self, node: &Node) -> bool` | 3 | Raised: impl doc describes `child_list.is_some()` check. |
| `isExpanded` (method) | 491 | PORTED | OK | `tv::Outline`'s `OutlineViewer::is_expanded(&self, node: &Node) -> bool` | 3 | Raised: impl doc says it returns `node.expanded`. |
| Destructor `~TOutline` | 491 | EQUIVALENT | OK | Automatic `Box<Node>` / `Drop` on `Outline` | N/A | C++: `~TOutline()` calls `disposeNode(root)`. Rust: `Box<Node>` drop is recursive; entire tree freed when `Outline` is dropped. No explicit destructor needed. Private/internal. |
| `TStreamable` / stream (read/write/readNode/writeNode) | — | NOT-PORTED | — | — | — | `TStreamable` / DOS stream machinery dropped project-wide (serde-if-revived). Includes `read`, `write`, `readNode`, `writeNode`, `build`. |
| `COutlineViewer` palette | 491 | EQUIVALENT | OK | `tv::theme::Role::OutlineNormal` / `OutlineFocused` / `OutlineSelected` / `OutlineNotExpanded` | 3 | Raised: all four Role variants now document when they apply (normal/focused/selected/collapsed rows) and the `ov_draw` context in which they are used. Edited in `src/theme.rs`. |

## Summary

- PORTED: 10   EQUIVALENT: 2   NOT-PORTED: 1   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable findings: All previously score-2 public symbols raised to score 3. The palette Role variants (`OutlineNormal` / `OutlineFocused` / `OutlineSelected` / `OutlineNotExpanded`) live in `src/theme.rs` rather than `outline.rs`; their docs were updated there. The `OutlineViewer` trait method implementations on `Outline` received impl-level doc comments covering the Outline-specific behavior (e.g. DFS-position keying for `adjust`).
