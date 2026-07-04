# InputLine caret ↔ screen-cursor desync: public offset fields with no resyncing setter

**Date:** 2026-07-04
**Status:** accepted & implemented — Options A (via cursor_request) + B +
anchor privatized; confirmed a porting deviation (C++ TInputLine::draw
re-derives the cursor every paint and its curPos/firstPos/selStart/selEnd are
public; anchor is private in C++).
**Component:** `InputLine` (`src/widgets/input_line.rs`), touching
`ViewState::cursor` (`src/view/view.rs`).

---

## Summary

`InputLine` publishes its caret/selection offsets as **public mutable fields**,
but the *rendered* cursor position is a **separate** piece of state
(`ViewState::cursor`) that is only re-derived inside the **private**
`sync_cursor()`. `draw` does not re-derive it. So writing `cur_pos` (or the other
offsets) directly — the obvious thing to do given they are `pub` — updates the
logical model while leaving the hardware cursor stranded wherever the last
edit/`select_all` put it. There is **no public method** to move the caret to an
arbitrary offset and resync, so a consumer cannot cleanly fix the desync from the
outside.

## How it surfaced

edaptor's entry-form pane wants the caret at the **start** of a field when focus
lands on it (Turbo Vision select-alls a field on focus, leaving the caret at the
end over a full-value selection; edaptor collapses that so the first keystroke does
not wipe the value). It did:

```rust
il.cur_pos = 0;
il.first_pos = 0;
il.sel_start = 0;
il.sel_end = 0;
il.anchor = 0;
```

Logically correct — and `value()`/tests that read `cur_pos` all agreed the caret
was homed. But on screen the hardware cursor sat at the **end** of every field. The
event loop places the cursor from `View::cursor_request()` → `ViewState::cursor`,
which nobody updated: the last thing to touch it was the focus-time
`select_all(true, true)` (caret → end, `sync_cursor()` called), and the manual
`cur_pos = 0` never re-ran `sync_cursor()`.

The consumer-side fix was to route homing through `select_all(false, false)`
(which *does* call `sync_cursor()`), plus zeroing `first_pos`/`anchor` by hand.
That works only because the target offset is 0 — `select_all` can express "home"
(`false`) and "end" (`true`) and nothing in between.

## Where the state lives

- **Logical caret / selection** — public fields on `InputLine`
  (`input_line.rs`):
  - `cur_pos` (120), `first_pos` (123), `sel_start` (125), `sel_end` (127),
    `anchor` (129) — all `pub`, documented only as byte/column offsets.
- **Screen cursor** — `ViewState::cursor: Point` (`view.rs:439`), read by the loop
  via `View::cursor_request()` (`view.rs:1018`, returns `s.cursor` when
  `cursor_vis`).
- **The bridge** — `InputLine::sync_cursor()` (`input_line.rs:301`), **private**:

  ```rust
  fn sync_cursor(&mut self) {
      let x = self.displayed_pos(self.cur_pos) - self.first_pos + 1;
      self.state.set_cursor(x, 0);
  }
  ```

  Called from exactly four places: `new` (251), `select_all` (369), and the two
  edit paths (511, 554). **Not** from `draw` (722) — the doc comment on the field
  even notes cursor placement was deliberately "split out of `draw`".

## Why this is a framework inconsistency, not just a consumer bug

1. **Public fields with a hidden invariant.** `cur_pos` et al. are `pub` and
   documented as plain offsets, with **no** note that writing them requires a
   resync. Contrast the neighbouring `validator` field (`input_line.rs:138`), which
   *does* warn: "to swap it out after construction use `set_validator` rather than
   assigning to this field directly." The caret fields carry the same
   write-then-resync contract but state it nowhere and offer no setter.

2. **No public "move caret" API.** The only public methods that move the caret
   **and** resync are `select_all(enable, …)` — home or end only — and the implicit
   move inside `paste_text`. A consumer literally cannot set an arbitrary caret
   offset through the public surface. (This is why edaptor had to *abandon* a
   "restore the exact in-progress caret across a background repaint" approach and
   settle for homing: there was no way to re-apply a saved position and have it
   render.)

3. **`draw` reads a derived value it never refreshes.** The screen cursor is a
   cache of `(cur_pos, first_pos)` that is only invalidated on specific mutating
   methods. Any code path that changes those offsets by another route silently
   leaves the cache stale until the next edit.

## Options

### A — Resync in `draw` / `cursor_request` (make the desync impossible)
Have the cursor be re-derived at paint time instead of cached on mutation. Either
call `sync_cursor()` at the top of `draw`, or compute `x` inside a
`cursor_request` override. Then `state.cursor` is always consistent with
`cur_pos`/`first_pos` regardless of how they were set, and the private/public split
stops mattering.
- **Pro:** kills the whole class of bug; direct field writes "just work"; the
  edaptor `select_all(false, false)` workaround can revert to a plain assignment.
- **Con:** the original design intentionally moved cursor placement out of `draw`
  (the field doc comment says so) — presumably to keep `draw` render-only and let
  the loop read a precomputed value between redraws. Worth confirming that rationale
  still holds before undoing it; `cursor_request` (called by the loop, not `draw`)
  may be the intended seam and a natural home for the derivation.

### B — Add a public caret setter that resyncs (fills the API gap)
Add e.g. `set_cursor_pos(&mut self, pos: i32)` (clamp to `data` bounds, adjust
`first_pos` for visibility, call `sync_cursor()`), and a convenience `home(&mut
self)` / `end(&mut self)`. Consumers move the caret through it; direct field writes
remain "expert mode."
- **Pro:** small, additive, unlocks arbitrary-position restore (the use case
  edaptor had to drop); keeps the mutation-time-resync design.
- **Con:** the raw fields stay writable and still desync if poked directly — so it
  should ship **with** option D's doc note.

### C — Encapsulate the offsets (private + accessors)
Make `cur_pos`/`first_pos`/`sel_start`/`sel_end`/`anchor` private, exposing getters
and resyncing setters, mirroring the `validator` contract.
- **Pro:** the invariant becomes unbreakable.
- **Con:** breaking change for any consumer already reading/writing the fields;
  heavier than the problem warrants on its own.

### D — Doc-only stopgap
Add a note to the caret fields ("writing this directly does not move the on-screen
cursor; use `select_all`/`set_cursor_pos`, or call the resync path"). Cheap, but
leaves the footgun loaded.

## Recommendation

**A** is the clean structural fix — it removes the invariant instead of documenting
it — provided the "cursor placement out of `draw`" decision can be honoured by
deriving in `cursor_request` rather than `draw`. Pair it with **B** so arbitrary
caret positioning has a first-class API (edaptor's dropped "preserve exact caret
across repaint" feature depends on B), and fold in **D**'s field-doc note either
way. **C** only if a breaking pass over `InputLine`'s surface is already planned.

## Consumer-side status (edaptor)

edaptor already worked around this on branch `feat/shuttle-widget`
(`src/ui/panes/form.rs`, `place_cursor_home`) by homing via
`select_all(false, false)`. When a real fix lands here, that method can drop back to
a direct assignment (option A) or call the new setter (option B). No rush on the
edaptor side; this note is the upstream half.
