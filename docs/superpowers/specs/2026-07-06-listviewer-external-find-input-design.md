# `ListViewer` external find-query input — design spec

> **Status:** draft, pending review. **Date:** 2026-07-06.
> **Type:** rstv-original *extension*, layered on the
> [incremental find-and-highlight](2026-06-30-listviewer-incremental-find-design.md)
> feature. That spec gave the list an opt-in `query` that its **own** keystroke
> handler builds. This spec opens a **second input path**: let a host feed the
> query from somewhere other than the list's key events — e.g. an `InputLine`
> that sits above the list and owns the text — so a combobox can drive the
> list's incremental find without the list being focused.

## Why this exists

The motivating consumer is [edaptor](https://github.com/oposs/edaptor)'s `lookup`
widget — the `gidNumber` selector. It is an editable combobox: an `InputLine` on
top, a candidate `ListBox` below. The user types a group *name* to find the row,
then commits its numeric *value*. Today the dialog re-implements matching by hand
(a private `row_matches` filter and a `highlight_index` scan), duplicating logic
the list already owns via find mode.

The clean split is: the `InputLine` is the text source; the `ListBox` is the
incremental search. As the field changes, its text should drive the list's find
query — the same query keystrokes would build, just fed from the field instead of
from the list's own `evKeyDown`. The list then highlights the match (find mode's
job); the dialog reads the focused row to fill the field on selection (the
dialog's job).

The current find API cannot express this. `find_query()` reads the query and
`clear_find()` empties it, but there is **no public way to set it**. The query is
mutable only inside `find_route_key`, which runs only when the *list itself* is
focused and receives the key. A combobox keeps focus on the input field, so those
key events never reach the list.

This is a generic widget concern — any combobox, completion popup, or
"search-box-drives-list" layout wants the same seam — so it belongs on
`ListViewer` next to `find_query` / `clear_find`.

### Relation to the prior find spec

Nothing about keystroke-driven find changes. `find_route_key`, the highlight
draw, the `Filter`/`Highlight` modes, and the `LIST_FIND_CHANGED` broadcast all
stay as they are. This spec adds one host-callable mutator that reuses the exact
same change tail (broadcast + `on_query_changed`) those keystrokes already run.
The keystroke path and the external path become two front doors onto one query.

## Core model

Add a single host-callable setter that replaces the query wholesale:

- **`set_find_query(query, ctx)`** — the mirror image of `clear_find`. It replaces
  `lv.query` with the given text and runs the standard change tail. A whole-string
  setter (not per-char push) is the right grain for a text source: an `InputLine`
  already holds the complete string every time it changes, so the host hands over
  the full text and never has to replay individual edits or keep a char cursor in
  sync with the list.

Semantics, chosen to match `clear_find` and the keystroke path exactly:

1. **Off is a no-op.** When `find_mode == Off` the list has no query concept;
   `set_find_query` returns without touching anything.
2. **Change-guarded.** If the new text equals the current query, it is a no-op —
   no broadcast, no hook. Only an actual change notifies. (Same guard `clear_find`
   uses for the already-empty case.) This keeps a host that mirrors its field on
   every event from emitting redundant broadcasts.
3. **Notify on change.** On a real change it broadcasts `LIST_FIND_CHANGED` with
   this list's `ViewId` as `source` and calls `on_query_changed(ctx)` — identical
   to `find_after_change`, minus the `ev.clear()` (there is no event to consume).
   So in `Filter` mode the list re-derives its narrowed view; in `Highlight` mode
   the host's rows are unchanged and the new query simply re-highlights on the
   next draw.
4. **Empty clears.** `set_find_query("")` is exactly `clear_find` reached through
   the same door — in fact `clear_find` is now *defined* as `set_find_query("")`.
   Both leave `query` empty and (if it was non-empty) notify. The host does not
   need to special-case the empty field.

## Layering — where each piece lives

The whole change is in the shared `list_viewer` trait; no concrete widget changes
are required (`on_query_changed` is already overridden by `ListBox`/`SortedListBox`
for self-filter, and `set_find_query` calls it).

`set_find_query` becomes the **single query mutator** for the host-callable path.
Because `set_find_query("")` is definitionally identical to `clear_find` (for the
empty string the "query unchanged" guard and `clear_find`'s "already empty" guard
coincide), `clear_find` is re-expressed as a one-liner onto it rather than keeping
a second inline copy of the change tail. The broadcast+hook tail — which now has
three would-be call sites (`find_after_change`, `set_find_query`, and the old
`clear_find` body) — is factored into one private `find_notify(this, ctx)` helper
so parity between the keystroke path and the external path is structural, not
something a test has to police.

- **`ListViewer::set_find_query`** (new trait method, default impl) — the single
  host-callable mutator:

  ```rust
  /// Set the find query from an external source (e.g. a host InputLine that owns
  /// the text). No-op when find mode is Off or the query is unchanged; otherwise
  /// replaces the query, fires LIST_FIND_CHANGED (source = this list), and runs
  /// on_query_changed — the same change tail keystroke find runs. Passing "" is
  /// exactly clear_find reached through the same door.
  fn set_find_query(&mut self, query: &str, ctx: &mut Context) {
      if self.lv().find_mode == FindMode::Off || self.lv().query == query {
          return;
      }
      self.lv_mut().query = query.to_string();
      find_notify(self, ctx);
  }
  ```

- **`ListViewer::clear_find`** (existing trait method) — collapses to a delegation,
  losing its inline broadcast+hook copy:

  ```rust
  /// Clear the find query — the host-callable Esc equivalent. Equivalent to
  /// set_find_query("", ctx): no-op when find is Off or already empty, else fires
  /// LIST_FIND_CHANGED and runs on_query_changed.
  fn clear_find(&mut self, ctx: &mut Context) {
      self.set_find_query("", ctx);
  }
  ```

- **`find_notify`** (new private free fn) — the shared change tail, extracted from
  the current `find_after_change`. Note it is *only* the broadcast + hook;
  `find_after_change` keeps its own `ev.clear()` around it (there is no event to
  consume on the host-callable path, which is why `set_find_query`/`clear_find`
  call `find_notify` directly):

  ```rust
  fn find_notify<L: ListViewer + ?Sized>(this: &mut L, ctx: &mut Context) {
      let source = this.lv().state.id();
      ctx.broadcast(Command::LIST_FIND_CHANGED, source);
      this.on_query_changed(ctx);
  }

  fn find_after_change<L: ListViewer + ?Sized>(this: &mut L, ev: &mut Event, ctx: &mut Context) {
      find_notify(this, ctx);
      ev.clear();
  }
  ```

- **No behavioural change** to `find_route_key`, `draw`, `filtered_view`, the two
  `FindMode` concrete widgets, or the `LIST_FIND_CHANGED` command. The only touch
  to existing code is the two-copy → one-helper refactor of the change tail; the
  observable behaviour of the keystroke path and `clear_find` is unchanged.

## Focus policy (open question — recommend deferring to the host)

Find mode as it stands **highlights** the match but does **not** move `focused`
to it (keystroke find in `Highlight` mode leaves the cursor where it was; `Filter`
mode only clamps `focused` into the narrowed range). So after `set_find_query`,
the *focused* row is not necessarily the *matched* row.

For the edaptor combobox this matters because the dialog fills the field from the
**focused** row when the user steps into the list. Two ways to bridge it:

- **(Recommended) Host owns focus.** Keep `set_find_query` about the query only,
  and let the host move focus if it wants — edaptor already scans its candidate
  vector (`highlight_index`) and can call the existing `focus_item_num`. This
  keeps the new API single-purpose and changes no existing consumer's behaviour.
- **(Alternative) Add `focus_find_match(ctx) -> Option<i32>`.** A companion trait
  method that scans `0..range` for the first `find_match` hit, focuses it, and
  returns the index (`None` if no row matches). Convenient, but it is a distinct
  concern from *input* and would tempt a behaviour change to keystroke find for
  consistency. Defer unless a second consumer wants it.

Recommendation: ship only `set_find_query` now; treat `focus_find_match` as a
follow-up if a real need appears. This keeps the spec to the one thing the title
promises — *input*, not *focus*.

## Consumer sketch (edaptor `lookup` — not part of this crate)

For orientation only; the edaptor changes ship in edaptor and get their own task:

- Give the candidate `ListBox` `FindMode::Highlight` (rows are loaded once, so the
  host keeps them; no self-filter).
- On each `InputLine` change, feed the field text to the list:
  `list.set_find_query(&field_text, ctx)`. The list highlights the match.
- Delete the dialog's hand-rolled `row_matches` filtering; the list owns matching.
- Selection/commit (field ← focused row, OK-enable on a leading number) stays
  edaptor's logic — unchanged by this spec.
- The "one blank row above the input field" and the field/list two-way copy are
  edaptor-side cosmetics/logic, explicitly **out of scope** here.

## Testing

Shared core, headless `Context`, on a fake `ListViewer` with find enabled:

- `set_find_query("ab")` on an empty query sets `find_query() == Some("ab")` and
  fires exactly one `LIST_FIND_CHANGED` (source = the list's id).
- Setting the **same** text again fires **no** broadcast and runs no hook (the
  change guard).
- `set_find_query("")` on a non-empty query empties it and notifies once — same
  observable result as `clear_find`.
- `find_mode == Off`: `set_find_query("x")` is a total no-op (no query, no
  broadcast).
- **Filter mode (concrete `ListBox`):** `set_find_query` narrows the view via
  `on_query_changed` just as a typed query does; an equivalent typed vs. set query
  yields the same visible rows.
- **Parity:** a query reached by keystrokes and the same query reached by
  `set_find_query` leave identical `query` / `find_query()` / narrowed-view state.

## Non-goals / open questions

- **Not** a per-character push API (`push_find_char` / `find_backspace`). A
  whole-string setter subsumes it for a text source; add granular mutators only if
  a consumer genuinely streams characters with no full-string view.
- **Not** focus movement — see *Focus policy*. `set_find_query` sets the query and
  nothing else.
- **Not** any change to keystroke find, the highlight draw, or the two find modes.
  Purely additive.
- **Settled:** the shared broadcast+hook tail is factored into one private
  `find_notify` helper, and `clear_find` is folded into `set_find_query("")`, so a
  single mutator carries the change tail (parity is structural, not test-policed).
- Open: `focus_find_match` companion — deferred unless a second consumer wants it.
