# TOutline  (guide pp. 489–491)

Rust module(s): src/widgets/outline.rs   |   magiblot: include/tvision/outline.h / source/tvision/toutline.cpp

> `TOutline` is the concrete outline viewer over an owned `TNode` tree. It
> derives from `TOutlineViewer` and provides concrete implementations of all
> the abstract navigation virtuals. The guide documents it on pp. 489–491
> (overview) and p. 492 (field `root`). In Rust it is `tv::Outline`, a struct
> that implements `OutlineViewer` and `View`.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `root` (field) | 491 | PORTED | OK | `tv::Outline::root: Option<Box<Node>>` | 2 | C++ `TNode* root`. Rust: `Option<Box<Node>>` — `None` = empty tree, `Some` = owned root. Public in both. `Box` drop recursively frees the entire tree (faithfully replaces `~TOutline()` + `disposeNode(root)`). Doc explains what it is; "how to build and swap the tree" could be noted. |
| `Init` (constructor) | 491 | PORTED | OK | `tv::Outline::new(bounds, h, v, root) -> Outline` | 2 | C++ constructor calls `TOutlineViewer(…)` then `update()`. Rust `new` constructs `OutlineViewerState` but does NOT call `ov_update` — it cannot (no `Context` available). The module doc and `new` rustdoc both note the consumer must call `ov_update` after insert. Score 2: the "what" is there; the "why no update in new" note is in the doc but not explained in terms of the C++ difference. |
| `adjust` (method) | 491 | PORTED | OK | `tv::Outline`'s `OutlineViewer::adjust(pos: i32, expand: bool)` | 2 | C++: `adjust(TNode* node, Boolean expand)` — sets `node->expanded`. Rust: `adjust(pos, expand)` — walks DFS to find the node at `pos`, then sets its `expanded`. Behavior identical for well-formed calls; the DFS-position keying (vs node-pointer keying) is a deliberate Rust design choice documented in the `OutlineViewer` trait. Doc explains the signature; the trade-off vs. C++ is covered only at the trait level. |
| `getChild` (method) | 491 | PORTED | OK | `tv::Outline`'s `OutlineViewer::get_child<'a>(&'a self, node: &'a Node, i: i32) -> Option<&'a Node>` | 2 | C++ walks `childList->next` `i` times. Rust: identical walk on `child_list.as_deref()` → `n.next.as_deref()`. Returns `None` when `i` is out of range. |
| `getNext` (method) | 491 | PORTED | OK | `tv::Outline`'s `OutlineViewer::get_next<'a>(&'a self, node: &'a Node) -> Option<&'a Node>` | 2 | Trivially returns `node->next`. Rust: `node.next.as_deref()`. |
| `getNumChildren` (method) | 491 | PORTED | OK | `tv::Outline`'s `OutlineViewer::get_num_children(&self, node: &Node) -> i32` | 2 | Counts siblings via `child_list → next` chain. Identical in both. |
| `getRoot` (method) | 491 | PORTED | OK | `tv::Outline`'s `OutlineViewer::get_root(&self) -> Option<&Node>` | 2 | Returns `self.root.as_deref()`. |
| `getText` (method) | 491 | PORTED | OK | `tv::Outline`'s `OutlineViewer::get_text<'a>(&'a self, node: &'a Node) -> &'a str` | 2 | Returns `&node.text`. |
| `hasChildren` (method) | 491 | PORTED | OK | `tv::Outline`'s `OutlineViewer::has_children(&self, node: &Node) -> bool` | 2 | `node.child_list.is_some()`. |
| `isExpanded` (method) | 491 | PORTED | OK | `tv::Outline`'s `OutlineViewer::is_expanded(&self, node: &Node) -> bool` | 2 | `node.expanded`. |
| Destructor `~TOutline` | 491 | EQUIVALENT | OK | Automatic `Box<Node>` / `Drop` on `Outline` | N/A | C++: `~TOutline()` calls `disposeNode(root)`. Rust: `Box<Node>` drop is recursive; entire tree freed when `Outline` is dropped. No explicit destructor needed. Private/internal. |
| `TStreamable` / stream (read/write/readNode/writeNode) | — | NOT-PORTED | — | — | — | `TStreamable` / DOS stream machinery dropped project-wide (serde-if-revived). Includes `read`, `write`, `readNode`, `writeNode`, `build`. |
| `COutlineViewer` palette | 491 | EQUIVALENT | OK | `tv::theme::Role::OutlineNormal` / `OutlineFocused` / `OutlineSelected` / `OutlineNotExpanded` | 2 | C++ `cpOutlineViewer = "\x06\x07\x03\x08"` (4 entries). `TOutline` inherits this from `TOutlineViewer` via `getPalette`. Rust: `Role`-keyed theme; same known idiomatic mapping. |

## Summary

- PORTED: 10   EQUIVALENT: 2   NOT-PORTED: 1   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 11   |   → concept: 0
- Notable findings: No correctness gaps. The most significant deviation from C++ — that `TOutline::Init` calls `update()` immediately but `Outline::new` cannot (no `Context`) — is documented but only at score 2; a consumer reading only `Outline::new` rustdoc might not understand why the `ov_update` post-insert call is mandatory. A "# Panics / Contract" or "# Usage" block on `new` spelling this out would reach score 3.
