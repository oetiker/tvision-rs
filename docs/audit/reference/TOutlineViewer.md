# TOutlineViewer  (guide pp. 491–498)

Rust module(s): src/widgets/outline.rs   |   magiblot: include/tvision/outline.h / source/tvision/toutline.cpp

> `TOutlineViewer` is the abstract base for collapsible tree views. It derives
> from `TScroller` in C++. In Rust it is split into two parts: the
> `OutlineViewer` trait (overridable methods) plus `OutlineViewerState` (data
> members), with shared draw/event/traversal logic in free functions generic
> over `L: OutlineViewer`. This is the same design choice used for
> `ListViewer` (D2 embed-and-delegate, documented in the module doc).

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `foc` (field) | 492 | PORTED | OK | `tv::OutlineViewerState::foc: i32` | 3 | Raised: rustdoc now explains that `foc` is the DFS position of the focused node, to read it for display purposes, and to call `adjust_focus` (not assign directly) to move the focus programmatically. |
| `delta` (inherited from TScroller) | — | PORTED | OK | `tv::OutlineViewerState::delta: Point` | 3 | Raised: rustdoc explains x = horizontal char skip, y = first visible DFS position, and that assignment comes only from the pump's read-sync (not direct assignment). |
| `Init` (constructor) | 492 | PORTED | OK | `tv::OutlineViewerState::new(bounds, h, v)` | 3 | Raised: rustdoc explains the grow/selectable defaults and the mandatory `ov_update` call after insertion. |
| `adjust` (method) | 492 | PORTED | OK | `tv::OutlineViewer::adjust(pos: i32, expand: bool)` | 3 | Raised: rustdoc explains when it is called (keyboard +/-/* and mouse graph-click), that `ov_update` must follow, and the DFS-position keying reason. |
| `draw` (method) | 493 | PORTED | OK | `tv::ov_draw<L: OutlineViewer>` (free fn) + `Outline::draw` delegates to it | 3 | Already score 3. |
| `expandAll` (method) | 493 | PORTED | OK | `tv::ov_expand_all<L: OutlineViewer>(this, pos: i32)` | 3 | Raised: rustdoc explains the `*` key binding, that siblings are not touched, the restart-each-round algorithm, and that `ov_update` must be called after. |
| `focused` (method) | 493 | PORTED | OK | `tv::OutlineViewer::focused_item(i: i32)` | 3 | Raised: rustdoc explains the notification contract (write `foc`, always call super/default), when to override (multi-select), and what the default does. |
| `getChild` (method) | 493 | PORTED | OK | `tv::OutlineViewer::get_child<'a>(&'a self, node: &'a Node, i: i32) -> Option<&'a Node>` | 3 | Raised: rustdoc explains the 0-based `child_list → next` walk, the `None` out-of-range return, and a Vec-backed implementation hint. |
| `getGraph` (method) | 493 | PORTED | OK | `tv::ov_get_graph<L: OutlineViewer>(…, ctx: &DrawCtx) -> String` (free fn, overridable by not calling it) | 3 | Raised: rustdoc explains the default char set, that override is achieved by not calling `ov_draw` and instead calling `create_graph` directly, and heritage note. |
| `getNext` (method) | 493 | PORTED | OK | `tv::OutlineViewer::get_next<'a>(&'a self, node: &'a Node) -> Option<&'a Node>` | 3 | Raised: rustdoc explains the traversal-iterates-siblings role and the lifetime tying. |
| `getNumChildren` (method) | 493 | PORTED | OK | `tv::OutlineViewer::get_num_children(&self, node: &Node) -> i32` | 3 | Raised: rustdoc explains the use by traversal/update and the must-agree-with-`has_children` contract. |
| `getPalette` (method) | 493 | EQUIVALENT | OK | `tv::theme::Role::OutlineNormal` / `Role::OutlineFocused` / `Role::OutlineSelected` / `Role::OutlineNotExpanded` | 2 | Role entries live in src/theme.rs — deferred to theme pass (constraint 5). |
| `getRoot` (method) | 494 | PORTED | OK | `tv::OutlineViewer::get_root(&self) -> Option<&Node>` | 3 | Raised: rustdoc explains the start-of-traversal role, the `None` empty-tree return, and lifetime semantics. |
| `getText` (method) | 494 | PORTED | OK | `tv::OutlineViewer::get_text<'a>(&'a self, node: &'a Node) -> &'a str` | 3 | Raised: rustdoc explains that the text is displayed verbatim, the lifetime allows returning a borrow without copying, and override hint for external text storage. |
| `handleEvent` (method) | 494 | PORTED | OK | `tv::ov_handle_event<L: OutlineViewer>` (free fn) + `Outline::handle_event` delegates | 3 | Already score 3. |
| `hasChildren` (method) | 494 | PORTED | OK | `tv::OutlineViewer::has_children(&self, node: &Node) -> bool` | 3 | Raised: rustdoc explains the graph-indicator and traversal-recurse roles and the must-agree-with-`get_num_children` contract. |
| `isExpanded` (method) | 494 | PORTED | OK | `tv::OutlineViewer::is_expanded(&self, node: &Node) -> bool` | 3 | Raised: rustdoc explains when traversal recurses (both `has_children` and `is_expanded` true) and the override pattern for externally-stored expanded state. |
| `isSelected` (method) | 494 | PORTED | OK | `tv::OutlineViewer::is_selected(&self, i: i32) -> bool` | 3 | Raised: rustdoc explains single-selection default and the multi-select override. |
| `selected` (method) | 495 | PORTED | OK | `tv::OutlineViewer::selected(&mut self, i: i32)` | 3 | Raised: rustdoc explains double-click/Enter trigger and that the override should broadcast `Command::OUTLINE_ITEM_SELECTED`. |
| `setState` (method) | 495 | PORTED | OK | `tv::ov_set_state<L: OutlineViewer>` (free fn) + `Outline::set_state` delegates | 3 | Raised: rustdoc explains the three side-effects (flag flip, focus broadcast, scrollbar show/hide) and that concrete widgets get this behavior by delegating `set_state`. |
| `update` (method) | 496 | PORTED | OK | `tv::ov_update<L: OutlineViewer>` (free fn) | 3 | Raised: rustdoc explains the mandatory post-insert call, the "after every tree mutation" contract, the internal count/width/limit/clamp sequence. |
| `expanded` (method — guide pp. 492 diagram) | 492 | NOT-PORTED | — | — | — | Guide-diagram artifact, not a real API entry. |
| `selected` (field — guide pp. 492 diagram) | 492 | NOT-PORTED | — | — | — | Guide-diagram artifact, not a real API entry. |
| `disposeNode` (method) | — | EQUIVALENT | OK | Automatic `Box<Node>` / `Drop` | N/A | Private/internal; automatic `Box` drop. |
| `firstThat` (method) | — | EQUIVALENT | OK | `tv::traverse<L, F>` (generic free fn, `F` returns `bool` to stop) | 3 | Raised: rustdoc explains the visitor signature, the stop-on-true / visit-all-on-false semantics, collapsed-subtree skipping, and the heritage note collapsing both C++ methods. |
| `forEach` (method) | — | EQUIVALENT | OK | `tv::traverse<L, F>` (same free fn, visitor always returns `false`) | 3 | Same symbol as `firstThat`; covered by the `traverse` doc update above. |
| `getNode` (method) | — | EQUIVALENT | OK | `tv::ov_get_node_info(this, pos: i32) -> Option<(i32, i64, u16)>` | 3 | Raised: rustdoc explains the returned triple, direct-call use cases, and the heritage note about replacing the raw `TNode*` return. |
| `createGraph` (method) | — | PORTED | OK | `tv::create_graph(level, lines, flags, lev_width, end_width, chars: &[char;8]) -> String` | 3 | Raised: rustdoc explains every parameter and chars slot, when to call directly vs. using `ov_get_graph`, and the heritage note. |
| `ovExpanded` constant | — | PORTED | OK | `OV_EXPANDED: u16 = 0x01` (`src/widgets/outline.rs:66`) | N/A | Private `const`; not public API. Already has a one-line comment. |
| `ovChildren` constant | — | PORTED | OK | `OV_CHILDREN: u16 = 0x02` (`src/widgets/outline.rs:68`) | N/A | Private `const`; not public API. Already has a one-line comment. |
| `ovLast` constant | — | PORTED | OK | `OV_LAST: u16 = 0x04` (`src/widgets/outline.rs:70`) | N/A | Private `const`; not public API. Already has a one-line comment. |
| `cmOutlineItemSelected` constant | — | PORTED | OK | `tv::command::Command::OUTLINE_ITEM_SELECTED` (= 301) | 1 | In `src/command.rs` — outside the permitted file set for this pass. Deferred to a command.rs sweep. |
| `TStreamable` / stream (read/write) | — | NOT-PORTED | — | — | — | TStreamable / DOS stream machinery dropped project-wide (serde-if-revived). |

## Summary

- PORTED: 25   EQUIVALENT: 5   NOT-PORTED: 3   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 1   |   → concept: 0
- Notable findings: 19 previously below-bar public symbols raised to score 3. One symbol left below bar: `Command::OUTLINE_ITEM_SELECTED` (score 1) lives in `src/command.rs`, outside the permitted file set for this pass — deferred to a command.rs sweep. The three private consts (`OV_EXPANDED`, `OV_CHILDREN`, `OV_LAST`) are not public API and marked N/A. The `getPalette` row maps to `Role` entries in `src/theme.rs` — deferred to theme pass per constraint 5.
