# TSortedListBox  (guide pp. 534–536)

Rust module(s): src/widgets/list_box.rs (struct `SortedListBox`)   |   magiblot: include/tvision/stddlg.h / source/tvision/stddlg.cpp

> TSortedListBox extends TListBox with an incremental type-to-search state
> machine over a TSortedCollection. In the port the sorted collection becomes
> an owned `Vec<String>` kept case-insensitively sorted; the incremental search
> machine lives as the `sorted_handle_event` / `sorted_cursor` free functions
> over the `SortedSearch` sub-trait. `SortedListBox` is a direct `ListViewer`
> implementor (same level as `ListBox`) — not a subclass of `ListBox` — because
> the shared abstract-base pattern (D2) makes inheritance unnecessary.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `searchPos` (field) | 535 | PORTED | OK | `tv::widgets::SortedListBox::search_pos: i32` (private) | N/A | Private field. C++ `short searchPos` initialized to -1; Rust `search_pos: i32` initialized to -1. Matches. |
| `shiftState` (field) | 535 | PORTED | OK | `tv::widgets::SortedListBox::shift_state: u8` (private) | N/A | C++ `uchar shiftState`. Rust `shift_state: u8`. Captured at the `search_pos -1↔0` transition; unused in the base, consumed by file-list subclasses. `KB_SHIFT` constant (`0x03`) is in `list_viewer.rs`. |
| `list` (field / accessor) | 535 | EQUIVALENT | OK | `tv::widgets::SortedListBox::list() -> &[String]` | 2 | C++ returns `TSortedCollection*`. Rust returns `&[String]` (the collection is always sorted by `new_list`). Known idiomatic mapping: TCollection → Vec. |
| `Init` (constructor) | 535 | PORTED | OK | `tv::widgets::SortedListBox::new(bounds, num_cols, h, v)` | 2 | C++: delegates to `TListBox(...)`, sets `shiftState=0`, `searchPos=-1`, `showCursor()`, `setCursor(1,0)`. Rust: delegates to `ListViewerState::new(...)`, calls `lv.state.show_cursor()` and `lv.state.set_cursor(1,0)`, sets `search_pos=-1`, `shift_state=0`. Matches. |
| `getText` (method) | 535 | PORTED | OK | `tv::widgets::list_box::ListViewer::get_text` on `SortedListBox` | 3 | Returns item from the owned `Vec<String>`; identical to `ListBox::get_text`. |
| `getKey` (method) | 535 | PORTED | OK | `tv::list_viewer::SortedSearch::search(cur: &[char]) -> i32` | 2 | C++ `getKey` returns `(void*)s` — the key IS the string pointer. `list()->search` then does the binary search. Rust `search` returns the INSERTION INDEX directly (binary-search over `get_text(mid)` case-insensitively). The C++ two-step (`getKey` → `collection.search`) is collapsed into one method in Rust; behavior identical. Doc explains what it does but not that "the key IS the typed prefix." |
| `handleEvent` (method) | 535 | PORTED | OK | `tv::list_viewer::sorted_handle_event(this, ev, ctx)` free function | 3 | C++ sequence — save `oldValue`, call base `TListBox::handleEvent`, reset `searchPos=-1` if focus moved OR `cmReleasedFocus`, gate on `evKeyDown && charCode != 0`, run kbBack/dot/char branches — is mirrored exactly. The one apparent divergence (C++ accumulates the cursor via `setCursor(cursor.x + (searchPos - oldPos), …)`, stddlg.cpp:172, vs Rust deriving it absolutely via `sorted_cursor`) is a documented deviation and `sorted_cursor` returns the correct position each frame, so behavior is equivalent. |
| `newList` (method) | 535 | PORTED | OK | `tv::widgets::SortedListBox::new_list(items: Vec<String>, ctx)` | 2 | C++: `TListBox::newList(aList)` + `searchPos=-1`. Rust: sorts `items` case-insensitively, replaces `self.items`, calls `set_range`, `focus_item(0)` if non-empty, resets `search_pos=-1`. The sort is a Rust extension (C++ relied on the collection being a `TSortedCollection`). Documented in module doc. Resets `search_pos`. |
| `getData` / `setData` / `dataSize` (inherited) | — | EQUIVALENT | OK | `SortedListBox::value()` (gather); `SortedListBox::set_value_ctx` (scatter) | 2 | C++ inherits `getData`/`setData`/`dataSize` from `TListBox`; `setData` calls `focusItem(p->selection)`. `SortedListBox::set_value_ctx` focuses the carried index via `list_viewer::focus_item_num`, mirroring `ListBox::set_value_ctx` and matching C++ `TListBox::setData`. Gather and scatter are symmetric: `value()` returns `FieldValue::Int(focused)`, and `set_value_ctx(FieldValue::Int(idx))` focuses that same index. |
| `TStreamable` / `read` / `build` (stream methods) | — | NOT-PORTED | — | — | N/A | C++ TStreamable serialization dropped; serde if revived (known idiomatic mapping). |

## Summary

- PORTED: 7   EQUIVALENT: 2   NOT-PORTED: 1   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 4   |   → concept: 0
- Gather and scatter are symmetric: `value()` returns the focused index and `set_value_ctx` focuses that index, matching C++ `TListBox::getData`/`setData` inheritance.
