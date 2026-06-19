# TSortedListBox  (guide pp. 534–536)

Rust module(s): src/widgets/list_box.rs (struct `SortedListBox`)   |   magiblot: include/tvision/stddlg.h / source/tvision/stddlg.cpp

> TSortedListBox extends TListBox with an incremental type-to-search state
> machine over a TSortedCollection. In the port the sorted collection becomes
> an owned `Vec<String>` kept case-insensitively sorted; the incremental search
> machine lives as the `sorted_handle_event` / `sorted_cursor` free functions
> over the `SortedSearch` sub-trait. `SortedListBox` is a direct `ListViewer`
> implementor (same level as `ListBox`) — not a subclass of `ListBox` — because
> the shared abstract-base pattern makes inheritance unnecessary.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `searchPos` (field) | 535 | PORTED | OK | `tv::widgets::SortedListBox::search_pos: i32` (private) | N/A | Private field. C++ `short searchPos` initialized to -1; Rust `search_pos: i32` initialized to -1. Matches. |
| `shiftState` (field) | 535 | PORTED | OK | `tv::widgets::SortedListBox::shift_state: u8` (private) | N/A | C++ `uchar shiftState`. Rust `shift_state: u8`. Captured at the `search_pos -1↔0` transition; unused in the base, consumed by file-list subclasses. `KB_SHIFT` constant (`0x03`) is in `list_viewer.rs`. |
| `list` (field / accessor) | 535 | EQUIVALENT | OK | `tv::widgets::SortedListBox::list() -> &[String]` | 3 | Raised: doc now states the slice is always in case-insensitive sorted order (from last `new_list`) and directs callers to `new_list` for replacement. |
| `Init` (constructor) | 535 | PORTED | OK | `tv::widgets::SortedListBox::new(bounds, num_cols, h, v)` | 3 | Raised: doc explains cursor-at-col-1 behavior (type-to-search visual), scroll-bar wiring, and post-insert population requirement. |
| `getText` (method) | 535 | PORTED | OK | `tv::widgets::list_box::ListViewer::get_text` on `SortedListBox` | 3 | Returns item from the owned `Vec<String>`; identical to `ListBox::get_text`. |
| `getKey` (method) | 535 | PORTED | OK | `tv::list_viewer::SortedSearch::search(cur: &[char]) -> i32` | 3 | Raised: doc now states the key IS the typed prefix `cur`, that the return is the insertion-point index, and that subclasses can override `search` for alternative key derivation. |
| `handleEvent` (method) | 535 | PORTED | OK | `tv::list_viewer::sorted_handle_event(this, ev, ctx)` free function | 3 | C++ sequence — save `oldValue`, call base `TListBox::handleEvent`, reset `searchPos=-1` if focus moved OR `cmReleasedFocus`, gate on `evKeyDown && charCode != 0`, run kbBack/dot/char branches — is mirrored exactly. The one apparent divergence (C++ accumulates the cursor via `setCursor(cursor.x + (searchPos - oldPos), …)`, stddlg.cpp:172, vs Rust deriving it absolutely via `sorted_cursor`) is a documented deviation and `sorted_cursor` returns the correct position each frame, so behavior is equivalent. |
| `newList` (method) | 535 | PORTED | OK | `tv::widgets::SortedListBox::new_list(items: Vec<String>, ctx)` | 3 | Raised: doc now explains the in-place sort, scroll-bar republish, focus reset, search-state clear, and post-insert requirement. |
| `getData` / `setData` / `dataSize` (inherited) | — | EQUIVALENT | OK | `SortedListBox::value()` (gather); `SortedListBox::set_value_ctx` (scatter) | 3 | Raised: `value()` doc explains gather-index semantics and that the index is into the sorted vec; `set_value_ctx` doc explains scatter, clamping, and that `new_list` must be called first to repopulate. |
| `TStreamable` / `read` / `build` (stream methods) | — | NOT-PORTED | — | — | N/A | C++ TStreamable serialization dropped; serde if revived (known idiomatic mapping). |

## Summary

- PORTED: 7   EQUIVALENT: 2   NOT-PORTED: 1   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- All previously below-bar public symbols raised to score 3. Key improvements: `search` doc now explains the "key IS the typed prefix" contract and subclass extensibility; `new_list` doc clarifies the sort + reset behavior; `value`/`set_value_ctx` docs explain the gather/scatter round-trip with sorted-index semantics.
