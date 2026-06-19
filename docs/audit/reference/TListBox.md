# TListBox  (guide pp. 467–470)

Rust module(s): src/widgets/list_box.rs   |   magiblot: include/tvision/dialogs.h / source/tvision/tlistbox.cpp

> TListBox is the first concrete list-viewer: it owns a collection of strings
> and overrides `getText`. In the port the abstract-class hierarchy is replaced
> by the `ListViewer` trait (see TListViewer audit); `TListBox` maps to the
> `ListBox` struct that embeds `ListViewerState` and implements `ListViewer`.
> The `TCollection` field becomes a `Vec<String>`.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `list` (field / accessor) | 468 | EQUIVALENT | OK | `tv::widgets::ListBox::list() -> &[String]` | 3 | Raised: doc now explains what it returns AND that direct writes bypass scroll-bar range; callers must use `new_list` to replace items. |
| `items` / `selection` (TListBoxRec fields) | 468 | EQUIVALENT | OK | `tv::data::FieldValue::Int` (focused index as `value()`) + `Vec<String>` (managed via `new_list`) | 3 | Raised via `value()` + `set_value_ctx` docs: both now explain the gather/scatter contract and that the collection is configuration, not dialog data. |
| `Init` (constructor) | 468 | PORTED | OK | `tv::widgets::ListBox::new(bounds, num_cols, h, v)` | 3 | Raised: doc now explains scroll-bar wiring parameters and two-step post-insert protocol; added `rust,ignore` usage example. |
| `getText` (method) | 469 | PORTED | OK | `tv::widgets::list_box::ListViewer::get_text(item) -> String` | 3 | C++ `strncpy` from `items->at(item)`, EOS if `items == 0`. Rust returns `items.get(item as usize).cloned().unwrap_or_default()`. OOB returns empty string. Matches. |
| `newList` (method) | 469 | PORTED | OK | `tv::widgets::ListBox::new_list(items: Vec<String>, ctx)` | 3 | Raised: doc now explains post-insert requirement, focus-reset behavior, and how to restore a prior selection after repopulation. |
| `setRange` (method) | 469 | PORTED | OK | `tv::list_viewer::set_range(this, range, ctx)` free function (via `ListViewer` base) | 3 | Inherited from `TListViewer`. See TListViewer audit. |
| `getData` (method) | 469 | EQUIVALENT | OK | `tv::view::View::value() -> Option<FieldValue::Int>` | 3 | Raised: doc now explains that the return is the focused index (dialog gather), that the collection is not included, and when to call it. |
| `setData` (method) | 469 | EQUIVALENT | OK | `tv::view::View::set_value_ctx(FieldValue, ctx)` | 3 | Raised: doc now states explicitly that the item list is NOT replaced, that `new_list` must be called separately for repopulation, and that out-of-range is clamped. |
| `dataSize` (method) | 469 | EQUIVALENT | OK | `tv::data::FieldValue` (sizing is implicit in the value type) | N/A | C++ returns `sizeof(TListBoxRec)`. Rust `FieldValue`-based protocol has no `dataSize` call; the dialog gather/scatter is type-checked at runtime. Known idiomatic mapping: `getData`/`setData` → value protocol. Not a public symbol. |
| `TStreamable` / `write` / `read` / `build` (stream methods) | — | NOT-PORTED | — | — | N/A | C++ TStreamable serialization dropped; serde if revived (known idiomatic mapping). |

## Summary

- PORTED: 4   EQUIVALENT: 5   NOT-PORTED: 1   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- All previously below-bar public symbols raised to score 3. Key improvement: `set_value_ctx` now explicitly states that the item list is NOT replaced (unlike C++ `setData` which called `newList`), and `new_list` doc clarifies the post-insert wiring requirement.
