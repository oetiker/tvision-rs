# Window settable minimum size — design

**Date:** 2026-07-03
**Status:** approved (ready for implementation plan)

## Problem

A `Window` has a **fixed** minimum interactive size of `16×6`. A consumer that
embeds large mandatory content (e.g. edaptor's two-column `Shuttle` picker, which
needs ~`60×20` before its columns overlap) cannot raise that floor: the user can
drag the window's corner far below the size its content needs, and the content
spills outside / clips against the frame.

**This is a port-fidelity bug, not just a missing feature.** In the C++
original, every interactive clamp path reads `sizeLimits` **virtually** —
`TFrame::dragWindow` calls `owner->sizeLimits(min, max)`
(`source/tvision/tframe.cpp:138`), so a subclass override
(`TFileDialog::sizeLimits` = 49×19, `TEditWindow::sizeLimits` = 24×6) *does*
govern the mouse drag, keyboard resize, and zoom in upstream Turbo Vision. The
Rust port's static dispatch silently lost that behavior.

The `size_limits` override that a composing wrapper (`FileDialog`, `EditWindow`,
edaptor's `ObjectClassPicker`/`MultiPickerDialog`) can define does **not** help
the interactive drag. Two reasons:

1. **The drag reads the `Window`'s own `size_limits`, statically.**
   `Window::start_drag` (`src/window/window.rs:879`) calls
   `View::size_limits(self, …)` where `self` is a `&Window`; this resolves to
   `<Window as View>::size_limits` at compile time. There is no virtual dispatch
   up to a composing wrapper, so a wrapper's override is invisible here. The same
   is true for `locate`/`maximize`/`ZOOM` (`:797`, `:828`) and the keyboard-resize
   capture — all read `View::size_limits(self=Window)`.

2. **`Window::size_limits` hardcodes the min and discards the group's min**
   (`src/window/window.rs:1678`):
   ```rust
   fn size_limits(&self, owner_size: Point) -> (Point, Point) {
       let (_min, max) = self.group.size_limits(owner_size);
       (Point::new(16, 6), max)   // min is a literal; group min ignored
   }
   ```
   So neither the embedded group nor any child can propagate a larger minimum
   upward — `Group::size_limits` is the trait default (min `0×0`, no child
   aggregation) and it is thrown away anyway.

Net: there is currently **no** API to raise a window's interactive-resize floor,
and three in-repo consumers are broken in exactly this way today:

- **`FileDialog`** is growable and overrides `size_limits` to `49×19`
  (`src/dialog/filedlg.rs:2337`), yet can be dragged below `49×19`. It also
  carries a manual 49×19 clamp in its screen-relative resize handling
  (`filedlg.rs:2284`-`:2288`) with the floor duplicated as literals.
- **`ChDirDialog`** overrides `size_limits` to `48×18` (`filedlg.rs:2850`,
  porting `TChDirDialog::sizeLimits`), same mechanism, same bug.
- **`EditWindow`** overrides `size_limits` to `24×6` (`src/widgets/editor.rs:3005`,
  mirroring `TEditWindow::sizeLimits`), yet an interactive drag ignores it and
  stops at `16×6`.

All three are in scope: converting them is the in-repo validation that the new
API covers the real (edaptor-shaped) use case.

## Root cause is a single chokepoint (this is the good news)

Every path that clamps the window's own size reads the min through
`View::size_limits(self=Window)`:

- `start_drag` → `DragCapture { min, … }` (`:879`, `:896`) — **mouse corner drag**
- the keyboard-resize capture — **keyboard resize** (captures min at entry)
- `locate` (`:828`) — **`ZOOM`, restore, fullscreen, programmatic resize**
- `calc_bounds` (via the trait default) — **owner-driven resize** (test
  `calc_bounds_honours_min_win_size`, `:1859`)

So making `Window::size_limits` return a **settable** minimum fixes *all* of them
at once, with no change to any call site.

## Decisions (locked with the user)

- Add a **settable per-window minimum** consulted by `Window::size_limits`; every
  clamp path picks it up automatically. This is the embed-and-delegate-compatible
  substitute for the C++ virtual `sizeLimits` override: a wrapper cannot be
  virtually dispatched to from inside the inner `Window`, so the wrapper *pushes
  its floor down* into the `Window` once, instead of the drag path *pulling* it
  up through virtual dispatch.
- Keep a **hard lower floor of `16×6`** (frame legibility: title + move/zoom/close
  icons). `set_min_size` clamps its argument up to at least `16×6`; it can only
  *raise* the floor, never lower it below what the chrome needs. (Mild deviation:
  a C++ override could in principle go *below* `minWinSize`; no upstream subclass
  does, and the chrome needs the floor — record in the deviation note.)
- **Default is `16×6`** — unchanged behavior for every existing window; this is a
  purely additive, backward-compatible change.
- **Surface on `Dialog` too** (the common consumer type), forwarding to the
  embedded `Window`, mirroring `Dialog::set_flags`.
- **Convert the in-repo consumers** (`FileDialog`, `ChDirDialog`, `EditWindow`)
  to the new mechanism in the same change: each sets its floor via
  `set_min_size` at construction and drops its now-redundant `size_limits`
  override, so there is a single source of truth. `FileDialog`'s manual clamp in
  the screen-relative resize **stays** (C++ applies that resize via
  `locate(bounds)`, `tfildlg.cpp:166`, which clamps through virtual
  `sizeLimits`; the Rust `request_bounds` deferred applies raw `change_bounds`
  with no clamp) — but it reads the floor from `View::size_limits` instead of
  duplicating the `49`/`19` literals.

## Design

### 1. `Window` — new field + setter

Add a field to `Window` (`src/window/window.rs`, alongside `flags`,
`restore_rect`, `bordered`):

```rust
/// The interactive-resize floor. Every size clamp (drag, keyboard resize,
/// ZOOM/restore, owner-driven calc_bounds) reads this through `size_limits`.
/// Defaults to the frame-legibility floor `MIN_WIN_SIZE` (16×6) and can only
/// be raised, never lowered below it.
min_size: Point,
```

Introduce a named constant for the existing literal (replaces the scattered
`Point::new(16, 6)`):

```rust
/// The absolute minimum a framed window may shrink to and still render its
/// title bar and corner icons legibly.
pub const MIN_WIN_SIZE: Point = Point::new(16, 6);
```

Initialize `min_size: MIN_WIN_SIZE` in `Window::new` (and any other constructor).

Setter + builder (place with the other `set_*`/`with_*` pairs, e.g. near
`set_drag_mode`/`with_drag_mode`):

```rust
/// Raise the window's interactive-resize floor. The argument is clamped up to
/// at least [`MIN_WIN_SIZE`] (title/icons must stay legible), so this can only
/// grow the minimum. Consulted by every size-clamp path via [`size_limits`].
pub fn set_min_size(&mut self, min: Point) {
    self.min_size = Point::new(min.x.max(MIN_WIN_SIZE.x), min.y.max(MIN_WIN_SIZE.y));
}

/// Builder form of [`set_min_size`](Self::set_min_size).
pub fn with_min_size(mut self, min: Point) -> Self {
    self.set_min_size(min);
    self
}
```

### 2. `Window::size_limits` — return the field

`src/window/window.rs:1678`:

```rust
fn size_limits(&self, owner_size: Point) -> (Point, Point) {
    let (_min, max) = self.group.size_limits(owner_size);
    (self.min_size, max)          // was: Point::new(16, 6)
}
```

No other call site changes — `start_drag`, `locate`, the keyboard-resize capture,
and `calc_bounds` all already read through this.

### 3. `Dialog` — forward the setter

`src/dialog/dialog.rs`, mirroring `set_flags` (`:145`):

```rust
/// Raise the dialog's interactive-resize floor (forwards to the embedded
/// [`Window`]; see [`Window::set_min_size`]).
pub fn set_min_size(&mut self, min: Point) {
    self.window.set_min_size(min);
}
```

Add the `with_min_size` builder too — `dialog.rs` keeps full `set_*`/`with_*`
parity (`flags`, `palette`, `grow_mode`, `drag_mode` all have both forms).

### 4. Convert `FileDialog` and `ChDirDialog` (`src/dialog/filedlg.rs`)

- `FileDialog::new` (`:1993`): call `dialog.set_min_size(Point::new(49, 19))`
  right after the growable-flags block. `ChDirDialog::new` (`:2581`): same with
  `Point::new(48, 18)`.
- Remove both `size_limits` overrides (`:2337`, `:2850`) and drop `size_limits`
  from both `#[delegate]` skip lists (`:2235`, `:2792`) — the macro then
  forwards to the embedded `Dialog`, whose window carries the floor; every path
  (including the interactive drag, previously broken) reads it.
- The manual clamp in the screen-relative resize (`:2284`-`:2288`) stays but
  reads `View::size_limits(self, screen_size).0` instead of the literal
  `49`/`19` (see Decisions — it emulates the C++ `locate()` clamp that the raw
  `change_bounds` deferred does not perform).
- Keep/adjust the existing `size_limits` tests: the observable
  `View::size_limits(&file_dialog, owner).0 == (49, 19)` contract is unchanged
  (and these assertions are what catch a forgotten `set_min_size` call once the
  overrides are gone).

### 5. Convert `EditWindow` (`src/widgets/editor.rs`)

- Call `set_min_size(Point::new(24, 6))` (the `minEditWinSize` port) on the
  embedded window at construction; introduce a `MIN_EDIT_WIN_SIZE` constant
  mirroring the C++ name if one doesn't exist yet.
- Remove the `size_limits` override (`:3005`), adjusting the `#[delegate]` skip
  list.
- The existing `edit_window_size_limits_min` test (`:4371`) keeps passing —
  same observable contract, now also honored by the interactive drag.

### 6. Doc-comment cleanup

Update the doc comments that state the min as a fixed `16×6` to say "defaults to
`MIN_WIN_SIZE` (16×6), raisable via `set_min_size`":

- `maximize` doc (`:790`, "min = 16×6")
- `size_limits` override doc (`:1670`-`:1677`)
- any "window minimum {16,6}" wording near the size-limit tests

### 7. Bookkeeping

- **`CHANGELOG.md`**: bullet under `## Unreleased` → `### New` (settable window
  minimum) and `### Fixed` (FileDialog/ChDirDialog/EditWindow interactive drag
  now honors their documented minimums).
- **`docs/PORTING-GUIDE.md`**: record the deviation — C++ reaches the floor via
  virtual `sizeLimits` overrides (pull); the Rust port uses a settable
  `min_size` field pushed down by the wrapper (D2 embed model has no upward
  dispatch). Note the hard `16×6` clamp as part of the same entry.

## Backward compatibility

Additive for the public surface: every window not calling `set_min_size` keeps
`min_size == MIN_WIN_SIZE`, so `size_limits` returns the same `(16×6, max)` as
today. Existing tests that assert the `16×6` default (`title_and_size_limits`
`:1845`, `calc_bounds_honours_min_win_size` `:1859`) remain valid.

The `FileDialog`/`ChDirDialog`/`EditWindow` conversions keep each type's
observable `size_limits` contract identical (49×19 / 48×18 / 24×6) — the
*behavior change* is that the interactive drag now honors those minimums, which
is the C++ behavior being restored (a bug fix, not a break).

## Testing

Unit tests in `src/window/window.rs` (headless, the existing size-limit test
style):

1. **Default unchanged** — a fresh `Window` reports `size_limits(...).0 ==
   MIN_WIN_SIZE`. (Covered by the existing default tests; keep them.)
2. **`set_min_size` raises the floor** — after `set_min_size(Point::new(60, 20))`,
   `size_limits(large_owner).0 == Point::new(60, 20)`.
3. **`set_min_size` cannot go below `MIN_WIN_SIZE`** — after
   `set_min_size(Point::new(4, 2))`, the floor stays `MIN_WIN_SIZE`.
4. **The floor governs an owner-driven clamp** — extend
   `calc_bounds_honours_min_win_size`: with `set_min_size(60, 20)`, a
   `calc_bounds`/`locate` that would shrink below `60×20` is clamped to it.
5. **The floor governs an interactive drag** — drive the `DragCapture` path (as
   the existing drag tests do): with a raised min, a corner drag toward zero stops
   at the raised floor, not at `16×6`.

`Dialog`: one test that `Dialog::set_min_size` forwards (raised floor visible via
`View::size_limits(&dialog, owner)`).

Consumer conversions:

6. **Converted consumers report their floors through the forwarded path** —
   `View::size_limits(&FileDialog, owner).0 == (49, 19)`, `ChDirDialog` →
   `(48, 18)`, `EditWindow` → `(24, 6)` (existing assertions; after the
   overrides are removed these catch a forgotten `set_min_size`). The
   interactive-drag wiring itself is proven once, generically, by test 5 — the
   drag reads the same `Window::size_limits` for every consumer.
7. **`EditWindow` drag honors 24×6** — one end-to-end pump-level drag on a
   converted consumer (the `drag_move_round_trip` style in `program.rs`): a
   corner drag toward zero stops at `24×6`, not `16×6` (this was the broken
   case).

`cargo fmt --all --check` + `cargo clippy --workspace --all-targets -- -D
warnings` + `cargo test --workspace` green.

## Consumer follow-up (edaptor — out of scope for this repo)

Once released, edaptor drops its ineffective wrapper `size_limits` overrides and
instead calls, in each picker builder (`oc_picker.rs`, `multi_picker.rs`) right
after `Dialog::new`:

```rust
dlg.set_min_size(Point::new(Shuttle::MIN_W, Shuttle::MIN_H)); // 60 × 20
```

`Shuttle::MIN_W`/`MIN_H` are already `pub(crate)`. This is the single source of
truth for the picker minimum; the wrapper-level `Shuttle::dialog_size_limits`
helper and the two `size_limits` overrides can then be removed.

## Out of scope

- Aggregating child `size_limits` up through `Group` (a bigger, separate change;
  the explicit per-window setter is simpler and sufficient here).
- Any change to `max` (still the owner size).
- A `min_size` on bare `View`/`ViewState` (only framed `Window`s need a raisable
  floor).
