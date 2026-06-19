# TMemo  (guide pp. 475–477)

Rust module(s): src/widgets/editor.rs (`struct Memo`)   |   magiblot: include/tvision/editors.h / source/tvision/tmemo.cpp

> TMemo is a subclass of TEditor that adds dialog-data exchange (`getData`/`setData`/`dataSize`) and
> palette (`getPalette`), and overrides `handleEvent` to swallow the Tab key.
> No TMemo-specific fields beyond what TEditor provides.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `Init` (constructor) | 475 | PORTED | OK | `tv::Memo::new(bounds, h_scroll_bar, v_scroll_bar, indicator, buf_size)` | 3 | Raised: doc now explains buf_size fixed-buffer semantics, how to pass None for absent siblings, and directs to `Group::insert_child` + scatter/gather for typical use. |
| `getData` (method) | 475 | EQUIVALENT | OK | `tv::Memo::value() -> Option<FieldValue>` (D10 value protocol) | 3 | Raised: doc now explains the return type, that it is called automatically by the dialog scatter pass, and directs callers to prefer `Dialog::gather`. |
| `setData` (method) | 476 | EQUIVALENT | OK | `tv::Memo::set_value(FieldValue)` (D10 value protocol) | 3 | Raised: doc now explains the Text-only contract, the silent-ignore behavior for non-Text variants, and directs callers to prefer `Dialog::scatter`. |
| `dataSize` (method) | 476 | EQUIVALENT | OK | size is implicit in `FieldValue::Text(String)` | N/A | C++: returns `bufSize + sizeof(ushort)` — the allocation for the record that `getData` fills. In D10 there is no separate size query; the `FieldValue` owns its allocation. No public counterpart needed. NOT public in Rust. |
| `getPalette` (method) | 476 | EQUIVALENT | OK | colors inherited from `tv::Editor` via `#[delegate(to = editor)]` → `Role::ScrollerNormal` / `Role::ScrollerSelected` | N/A | C++: `cpMemo = "\x1A\x1B"` (2 entries, same as editor normal/selected). Rust `Memo` has no `palette()` override; it delegates to `Editor`, which uses `Role::ScrollerNormal`/`ScrollerSelected` color lookup. The module doc says "reuses the editor's drawing and so its scroller colors; it carries no separate palette of its own." Functionally equivalent. Known idiomatic mapping: class Palette → `tv::Theme`. |
| `handleEvent` (method) | 476 | PORTED | OK | `tv::Memo::handle_event` (impl `View::handle_event` in `#[delegate]` block) | 3 | C++: `if (event.what != evKeyDown || event.keyDown.keyCode != kbTab) TEditor::handleEvent(event);` — swallows only plain Tab, forwarding all else. Rust: identical logic: returns early (without clearing) on an unmodified Tab `KeyDown`; all other events forwarded to `editor.handle_event`. Comment explains Shift/Ctrl/Alt+Tab ARE forwarded; test `memo_tab_swallowed_not_cleared` verifies the swallow-without-clear. Full match. |
| `TMemoData` type | 477 | EQUIVALENT | OK | `tv::data::FieldValue::Text(String)` | N/A | See dedicated TMemoData.md. Cross-reference only. |
| `Load` (stream constructor) | 477 | NOT-PORTED | — | — | — | `TStreamable` / stream machinery dropped project-wide (serde-if-revived). Known idiomatic mapping. |
| `Store` (stream method) | 477 | NOT-PORTED | — | — | — | Same: TStreamable dropped. Known idiomatic mapping. |

## Summary

- PORTED: 2   EQUIVALENT: 5   NOT-PORTED: 2   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable findings: All 3 previously below-bar public symbols raised to score 3 (`new`, `value`, `set_value`). `handleEvent` was already score 3. `dataSize` and `getPalette` have no public Rust counterpart (N/A — the D10 typed value protocol subsumes `dataSize`; palette is folded into the editor's theme delegation). The `getData`/`setData` → `value`/`set_value` D10 mapping is documented on each method.
