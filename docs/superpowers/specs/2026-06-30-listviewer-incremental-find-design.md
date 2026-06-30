# `ListViewer` incremental find-and-highlight — design spec

> **Status:** design approved (brainstorm complete), pending implementation plan.
> **Date:** 2026-06-30. **Type:** rstv-original *extension* on a faithful base.
> Turbo Vision's `TListViewer` already has an incremental **type-to-search**
> ("lookup") that jumps the focus to the first item whose text matches the typed
> prefix. This extends that machinery from *jump-to-prefix* into *find* — an
> accumulated query that (optionally) **filters** the visible rows and
> **highlights** the matched substring in place — so a consumer no longer needs a
> separate search `InputLine` stacked above the list.

## Why this exists

The motivating consumer is [edaptor](https://github.com/oposs/edaptor), which today
stacks a search `InputLine` (a "Filter:" box) above two different lists — the entry
browser and the two-column `Shuttle` (object-class / membership editor). The box
costs one-to-three rows of vertical space at the top of every such list, and it
duplicates state the list could own itself: the list already has focus, already
draws every row, and (for static data) already holds every candidate.

The ask: **let the list own the search.** Start typing while the list is focused;
the query narrows the list and highlights what matched; no box, no reserved rows.

This is a generic widget concern, not an application one — the same interaction is
wanted by any sorted picker, file list, or completion list — so it belongs in
`ListViewer`, alongside the existing lookup it generalises.

### Turbo Vision heritage

`TListViewer::handleEvent` (`tvlistv.cpp`) implements an incremental search on
`evKeyDown`: it re-seeds a working string from the **focused item's** text each
keystroke and advances a `searchPos`, jumping focus to the next item that still
matches (Backspace walks it back). rstv ports this as the `SortedSearch` /
`sorted_handle_event` state machine. The port is faithful: it **moves focus**, it
does **not** filter and does **not** highlight.

The extension keeps that lookup as the default and adds an opt-in **find mode**
that swaps the "seed-from-focused-item, jump" behaviour for "accumulate a typed
query, optionally filter, always highlight". Nothing about the default behaviour
changes for existing consumers.

## Core model

Find is a per-list **opt-in** with two independent switches:

1. **find mode** (the always-on core when enabled): the list owns a `query`
   String. Printable keys append; Backspace deletes the last char; Esc clears.
   While a non-empty query exists, every visible row is drawn with the first
   case-insensitive occurrence of the query painted in an accent colour. The
   query is exposed (`find_query()`), and a **change broadcast** fires (the
   list's own `ViewId` as `source`) whenever it changes — the same notify-by-
   broadcast pattern `ScrollBar` uses. Arrow / page / Home / End keys, Space-
   select vs. Space-as-query, and Enter are addressed under *Key routing* below.

2. **self-filter** (opt-in, layered on find mode): the list keeps the full set it
   was given as a **source** and displays only the rows whose text contains the
   query (case-insensitive substring). The host hands over the complete list once
   (and again whenever the underlying data changes) and does nothing per
   keystroke. Without this switch the displayed rows are exactly what the host fed
   (`new_list`), and the list only owns the query + highlight — for hosts whose
   candidate set is produced externally (e.g. a live async search that can never
   hold every row).

When find mode is enabled and a query filters the view to **empty**, the empty-
area placeholder reads `No match: <query>` instead of the stock `<empty>`, so an
over-typed or mistyped query is always visible even though there is no dedicated
query line. (This is the *only* place the raw query is rendered as text; while
rows survive, the highlight is the query's on-screen presence.)

## Layering — where each piece lives

The feature spans the shared `list_viewer` core and the concrete widgets,
following the existing split (shared trait functions call back into concrete
`get_text` / item storage):

- **`ListViewerState`** gains an optional find substate: `{ enabled, self_filter,
  query: String }` plus the search-position bookkeeping find replaces. A trait
  accessor `find_query() -> Option<&str>` lets the shared `draw` reach it.
- **`list_viewer::handle_event`** (shared): when find mode is enabled, route
  printable keys / Backspace / Esc into the query instead of the lookup state
  machine, fire the change broadcast on any change, and call an
  `on_query_changed` hook (default no-op) so a self-filtering concrete widget can
  re-derive its view. Non-query keys (arrows, etc.) fall through unchanged.
- **`list_viewer::draw`** (shared): the single change to rendering. Today each row
  is one `put_str_part(.., color)`. With a non-empty `find_query()`, split the row
  text at the first case-insensitive match into before / match / after and emit
  three `put_str_part` calls — the match span in an accent role, the rest in the
  row's normal/selected/focused colour. The horizontal-scroll `indent` and the
  per-column geometry are unchanged; only the text-drawing line is replaced.
- **`ListBox` / `SortedListBox`** (concrete): self-filter storage. `new_list`
  sets the **source**; the displayed view (what `get_text`/`range` expose) is the
  source narrowed by the query. `on_query_changed` re-derives the filtered view
  and clamps `focused`/`top_item`. `SortedListBox` filters its already-sorted
  source; `ListBox` filters in source order. Pass-through (find without
  self-filter) leaves `new_list` semantics exactly as today.

## API surface (illustrative — final names settled in the plan)

```rust
// Enable on construction or via a setter; both default OFF (today's lookup).
ListBox::new(bounds, num_cols, h, v).with_find(FindMode::Filter)   // self-filtering
SortedListBox::new(bounds, num_cols, h, v).with_find(FindMode::Highlight) // query+highlight only

enum FindMode { Off, Highlight /* query + highlight, host filters */, Filter /* + self-filter */ }

impl ListViewer {
    fn find_query(&self) -> Option<&str>;   // None when find is Off or query empty
    fn clear_find(&mut self, ctx);          // Esc equivalent, callable by the host
}

// Notify-by-broadcast, source = the list's ViewId (mirrors ScrollBar):
pub const LIST_FIND_CHANGED: Command = Command::custom("listviewer.find.changed");
```

## Key routing (find mode active + the list focused)

- **Printable char** → append to query (this includes Space, so multi-word
  queries work). This supersedes Space-as-select while find mode is active; a
  host that needs Space-select with find is out of scope (none do).
- **Backspace** → delete last query char; on an empty query, no-op.
- **Esc** → clear the query (does *not* propagate as a dialog cancel while a query
  is non-empty; an empty-query Esc passes through so a host dialog still closes).
- **Up / Down / PageUp / PageDown / Home / End / Enter / Tab** → unchanged; they
  navigate / select / move / traverse exactly as today. The query persists across
  navigation.

## Consumer validation (edaptor — the proving ground, not part of this crate)

The design is validated against three real consumers; their integration ships in
edaptor, but it is what shapes the API:

- **Entry browser (static, in memory):** find + **self-filter**. Delete the
  `Filter:` box; the list holds the node's full entry set and narrows itself.
- **object-class editor (static):** find + **self-filter** on the *Available*
  column. The host re-feeds the source (all classes minus the active set) when a
  class is moved; the list filters by its own query. Delete the column's box.
- **membership editor (async):** find **without** self-filter on the *Available*
  column. The list owns the query and broadcasts changes; the host submits the
  LDAP search and re-feeds results as they arrive; the list highlights whatever it
  is shown. Delete the column's box.

That all three fall out of *one* feature (with self-filter as the only switch
between static and async) is the evidence the boundary is right.

## Testing

- **Shared core (headless `Context`):** query accumulation, Backspace, Esc-clear;
  the `LIST_FIND_CHANGED` broadcast fires on change and only on change; non-query
  keys still navigate.
- **Highlight (pure):** the before/match/after split for a row + query (no match,
  match at start/middle/end, case-insensitive, multibyte-safe char indexing).
- **Self-filter (concrete):** a query narrows the view, clears restore the full
  source, `focused`/`top_item` stay in range; `SortedListBox` filters its sorted
  order, `ListBox` its insertion order.
- **Empty state:** an all-filtering query yields the `No match: <query>`
  placeholder, not `<empty>`.

## Non-goals / open questions

- **Not** fuzzy matching, regex, or multi-term — plain case-insensitive substring.
  (A later `MatchKind` could extend `FindMode`; out of scope now.)
- **Not** a replacement for the default lookup — that stays the default; find is
  opt-in so no existing consumer changes.
- Highlight is the **first** occurrence per row. All-occurrences highlighting is a
  cheap later extension to the same draw split; deferred unless wanted.
- Accent role: reuse an existing theme role (e.g. the focused/hot-key family)
  rather than introduce a new palette entry, pending a look at `Theme`.
