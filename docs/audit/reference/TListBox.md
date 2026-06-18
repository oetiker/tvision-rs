# TListBox  (guide pp. 467–470)

Rust module(s): src/widgets/list_box.rs   |   magiblot: include/tvision/dialogs.h / source/tvision/tlistbox.cpp

> TListBox is the first concrete list-viewer: it owns a collection of strings
> and overrides `getText`. In the port the abstract-class hierarchy is replaced
> by the `ListViewer` trait (see TListViewer audit); `TListBox` maps to the
> `ListBox` struct that embeds `ListViewerState` and implements `ListViewer`.
> The `TCollection` field becomes a `Vec<String>`.

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `list` (field / accessor) | 468 | EQUIVALENT | OK | `tv::widgets::ListBox::list() -> &[String]` | 2 | C++ `items: TCollection*` (private) exposed via inline `list()` returning `TCollection*`. Rust `items: Vec<String>` (private) exposed via `list() -> &[String]`. Known idiomatic mapping: TCollection family → `Vec`. Doc explains what it returns but not why the collection is configuration (vs. the `value` protocol). |
| `items` / `selection` (TListBoxRec fields) | 468 | EQUIVALENT | OK | `tv::data::FieldValue::Int` (focused index as `value()`) + `Vec<String>` (managed via `new_list`) | 2 | C++ `TListBoxRec { items: TCollection*; selection: ushort }` is the dialog record shape. Rust splits this: `value()` returns `FieldValue::Int(focused)` (the selection); the item collection is NOT part of the value protocol — it is configuration managed via `new_list`. Module doc explains the split. `FieldValue` doc is adequate. |
| `Init` (constructor) | 468 | PORTED | OK | `tv::widgets::ListBox::new(bounds, num_cols, h, v)` | 2 | C++ ctor: `TListViewer(bounds, aNumCols, 0, aScrollBar)` + `setRange(0)`. Rust: constructs `ListViewerState::new(...)` + empty `Vec`. Note: C++ takes only one `TScrollBar*` (vScrollBar); Rust takes both `h` and `v` explicitly (consistent with `ListViewerState`). The guide omits hScrollBar in TListBox's constructor — this is fine because C++ hardcodes `0` for hScrollBar. Rust's extra `h` parameter is harmless; no comment. |
| `getText` (method) | 469 | PORTED | OK | `tv::widgets::list_box::ListViewer::get_text(item) -> String` | 3 | C++ `strncpy` from `items->at(item)`, EOS if `items == 0`. Rust returns `items.get(item as usize).cloned().unwrap_or_default()`. OOB returns empty string. Matches. |
| `newList` (method) | 469 | PORTED | OK | `tv::widgets::ListBox::new_list(items: Vec<String>, ctx)` | 3 | C++: `destroy(items)`, set `items = aList`, `setRange(count)`, `focusItem(0)` if non-empty, `drawView()`. Rust: replaces `self.items`, calls `set_range`, `focus_item(0)` if non-empty. No `drawView()` — whole-tree redraw (D9). Module doc explains post-insert wiring. |
| `setRange` (method) | 469 | PORTED | OK | `tv::list_viewer::set_range(this, range, ctx)` free function (via `ListViewer` base) | 3 | Inherited from `TListViewer`. See TListViewer audit. |
| `getData` (method) | 469 | EQUIVALENT | OK | `tv::view::View::value() -> Option<FieldValue::Int>` | 2 | C++ `getData` fills a `TListBoxRec { items, selection=focused }`. Rust `value()` returns `FieldValue::Int(focused)`. The item collection is NOT transferred (it is configuration). D10 value protocol. Doc explains what `value()` returns but could clarify why items are excluded. |
| `setData` (method) | 469 | EQUIVALENT | OK | `tv::view::View::set_value_ctx(FieldValue, ctx)` | 2 | C++ `setData` calls `newList(p->items)` + `focusItem(p->selection)` + `drawView()`. Rust `set_value_ctx` calls `focus_item_num(idx)` only — does NOT replace the item list (see getData mapping above). If a dialog scatter needs to repopulate the list it must call `new_list` separately. This is a documented design choice (D10), though the rustdoc does not say "items are excluded." |
| `dataSize` (method) | 469 | EQUIVALENT | OK | `tv::data::FieldValue` (sizing is implicit in the value type) | N/A | C++ returns `sizeof(TListBoxRec)`. Rust `FieldValue`-based protocol has no `dataSize` call; the dialog gather/scatter is type-checked at runtime. Known idiomatic mapping: `getData`/`setData` → value protocol. Not a public symbol. |
| `TStreamable` / `write` / `read` / `build` (stream methods) | — | NOT-PORTED | — | — | N/A | C++ TStreamable serialization dropped; serde if revived (known idiomatic mapping). |

## Summary

- PORTED: 4   EQUIVALENT: 5   NOT-PORTED: 1   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 6   |   → concept: 0
- Most important finding: No gaps or correctness bugs. The one design point worth noting in rustdoc: `set_value_ctx` does NOT replace the item list (unlike C++ `setData` which called `newList`); the doc should say so explicitly so callers know they must call `new_list` separately when repopulating a dialog.
