# Implementer brief — Row 30 `TDeskTop`, module `desktop`

You are porting **one class** of magiblot/tvision to idiomatic Rust in the
`tvision` crate (house alias `tv::`). This is a small **FOUNDATION** warm-up: a
`TGroup` subclass that owns a `TBackground` and (later) tiles/cascades windows.
Its real value here is to give `Program` a **named real desktop** and to
establish the **"a `View` that embeds a `Group` and delegates the `View` trait"**
pattern that `TWindow` (row 33) will copy.

Port **faithfully** from the C++; the only intentional departures are the
pre-decided deviations named below. Do **not** invent extra features and do
**not** leave dead stubs — defer cleanly (see "Deferrals").

C++ source of truth (read it):
`/home/oetiker/scratch/tvision-spec/magiblot-tvision/source/tvision/tdesktop.cpp`
and the class decl in `include/tvision/app.h` (`class TDeskTop : public TGroup,
public virtual TDeskInit`).

## What you are building

A new file **`src/desktop/desktop.rs`** with a `Desktop` struct, wired into the
existing `src/desktop/mod.rs` (add `mod desktop;` + `pub use desktop::Desktop;`)
and re-exported at the crate root in `src/lib.rs` (`pub use desktop::{Background,
Desktop};`, keep alphabetical). You are the only agent touching the tree — do all
wiring yourself.

### `Desktop` embeds a `Group` (D2 embed-and-delegate) and **is a `View`**

Unlike `Program` (which is *not* a `View`), `Desktop` **is** a `View` — it is a
child of the program's root group. It embeds a `Group` as its container and
delegates the whole `View` trait to it. This delegation boilerplate is the
exemplar `TWindow` will copy, so make it clean and complete.

```rust
pub struct Desktop {
    group: Group,
    /// The inserted background child's id — `TDeskTop::background`.
    /// Consumed by cmPrev's `putInFrontOf(background)` at row 33; exposed via
    /// `background()` now so the field stays live under `-D warnings`.
    background: Option<ViewId>,
}
```

Do **not** add a separate `ViewState` to `Desktop`; `state()`/`state_mut()`
return the inner `group`'s state (this is an *is-a* `TGroup` relationship — the
growMode below lives on the group's state).

### Construction — `Desktop::new(...)` ports `TDeskTop::TDeskTop` + `TDeskInit`

C++ ctor:
```cpp
TDeskTop::TDeskTop( const TRect& bounds ) :
    TDeskInit( &TDeskTop::initBackground ), TGroup(bounds)
{
    growMode = gfGrowHiX | gfGrowHiY;
    tileColumnsFirst = False;
    if( createBackground != 0 && (background = createBackground( getExtent() )) != 0 )
        insert( background );
}
```

Mirror `Program::new`'s **factory-mixin** injection (the `TDeskInit` background
factory):

```rust
pub fn new(bounds: Rect, create_background: impl FnOnce(Rect) -> Option<Box<dyn View>>) -> Self
```

Steps, faithful:
1. `let mut group = Group::new(bounds);`
2. Set `growMode = gfGrowHiX | gfGrowHiY` on the group's state
   (`group.state_mut().grow_mode.hi_x = true; .hi_y = true;`).
3. Call `create_background(group.state().get_extent())`; if it returns
   `Some(view)`, `insert` it into the group and record the returned `ViewId` in
   `self.background`. (`getExtent()` is the local-origin extent — see
   `ViewState::get_extent`.)
4. Drop `tileColumnsFirst` — it is only read by `tile` (deferred). Do **not** add
   the field (it would be dead under `-D warnings`).

Provide the default background factory (ports `TDeskTop::initBackground`):

```rust
/// `TDeskTop::initBackground` — the default background factory:
/// `new TBackground(r, defaultBkgrnd)`.
pub fn init_background(r: Rect) -> Box<dyn View> { ... }
```

**The fill character is faithful and non-negotiable:** C++
`TDeskTop::defaultBkgrnd == '\xB0'` (`tvtext2.cpp`). CP437 `0xB0` is **U+2591 ░
LIGHT SHADE** (the project's CP437 convention, same family as the scrollbar
shades in `theme.rs`: `0xB0`→░ `'\u{2591}'`, `0xB1`→▒ `'\u{2592}'`,
`0xB2`→▓ `'\u{2593}'`). Use `'\u{2591}'`. **Do not** copy the `'▒'` (U+2592) that
appears in existing row-29/31 *test scaffolding* — that was an arbitrary test
pick and is the wrong glyph for the desktop default.

Add a `background()` accessor:
```rust
/// `TDeskTop::background` — the background child's id (row 33's
/// `putInFrontOf(background)` target).
pub fn background(&self) -> Option<ViewId> { self.background }
```

### `View` impl — delegate every method to the inner `Group`

`TDeskTop` overrides only `handleEvent` (and the deferred `tile`/`cascade`/
`shutDown`/`tileError`). Everything else is inherited `TGroup`. So delegate the
full trait to `self.group`:

- `state` / `state_mut` → `self.group.state()` / `.state_mut()`
- `draw` → `self.group.draw(ctx)`
- `set_state` → `self.group.set_state(flag, enable, ctx)`
- `valid` → `self.group.valid(cmd)`
- `awaken` → `self.group.awaken()`
- `size_limits` → `self.group.size_limits(owner_size)`
- `calc_bounds` → `self.group.calc_bounds(owner_size, delta)`
- `change_bounds` → `self.group.change_bounds(bounds)`
- `cursor_request` → `self.group.cursor_request()`
- `handle_event` → **for row 30, also just delegate**: `self.group.handle_event(ev, ctx)`.
  See "Deferrals" for why the cmNext/cmPrev override is *not* implemented now.

Check the exact defaulted-vs-required method set in `src/view/view.rs` (the
`View` trait around line 557) and delegate whatever is there. If a method is
defaulted and the default (calling the base, e.g. `handle_event` no-op) is wrong
for a group, you must still delegate it to `self.group` — `Group` overrides these
meaningfully, and `Desktop` must forward to the group's override, not inherit the
trait default.

### Deferrals — defer cleanly, NO stubs (each would be dead code under `-D warnings`)

`TDeskTop::handleEvent` does, after `TGroup::handleEvent(event)`:
```cpp
if( event.what == evCommand )
    switch( event.message.command ) {
        case cmNext: if( valid(cmReleasedFocus) ) selectNext(False); break;
        case cmPrev: if( valid(cmReleasedFocus) ) current->putInFrontOf(background); break;
        default: return;          // <-- note: returns BEFORE clearEvent
    }
    clearEvent(event);            // reached ONLY for cmNext / cmPrev
```

Both cmNext and cmPrev depend on the Z-reorder machinery
(`selectNext`→`select()`→`ofTopSelect`/`makeFirst`; `putInFrontOf`) that is
**deferred to row 33 (`TWindow`)** per the project plan, and both commands start
**disabled** in `default_command_set` (program.rs) — there are no windows to
navigate at row 30, so the override has **zero observable effect** here.
Therefore: **do not** implement the cmNext/cmPrev override; `Desktop::handle_event`
just delegates to `self.group.handle_event`. Leave a precise breadcrumb comment
so row 33 restores it faithfully:

```rust
// TODO(row 33, D9): TDeskTop::handleEvent's command override. After delegating
// to the group, if event is a command:
//   cmNext: if valid(cmReleasedFocus) { selectNext(false) }   // findNext+select
//   cmPrev: if valid(cmReleasedFocus) { current.putInFrontOf(background) }  // Z-reorder
//   default: return WITHOUT clearing the event.
// clearEvent is reached ONLY for cmNext/cmPrev. Needs ofTopSelect/makeFirst/
// putInFrontOf (row 33) + numbered windows, so deferred whole.
```

Also defer (do **not** add fields/methods/skeletons — they would be dead):
- `tile` / `cascade` / `tileError` (the tiling geometry — `mostEqualDivisors` /
  `calcTileRect` / `doCascade`). Needs `ofTileable` + a `locate` path; lands when
  windows exist. One-line module-doc note is enough.
- `shutDown` (`background = 0; TGroup::shutDown()`) — no shutDown path yet.
- `tileColumnsFirst` field — only `tile` reads it.

Streamable `read`/`write`/`build`/`name` are dropped (D12) — do not port them.

## Program wiring — deliver the "named real desktop" (do this, it is in scope)

The handover's stated value of row 30 is to give `Program` a *real named desktop*
instead of the ad-hoc `Group`+`Background` the row-31 tests build. Deliver it:

1. **Production path:** confirm `Program::new`'s `create_desktop` factory can be
   fed `|r| Some(Box::new(Desktop::new(r, Desktop::init_background)))`. (No
   `Program` API change needed — just verify it composes; you may add a short
   doc example.)
2. **Migrate the row-31 test harness.** In `src/app/program.rs`, change
   `program_with_desktop`'s `create_desktop` closure from the hand-rolled
   `Group::new(r)` + `Background::new(.., '▒')` to
   `Some(Box::new(Desktop::new(r, Desktop::init_background)))`. This is the faithful
   desktop. It changes the desktop fill char ▒→░ in
   `pump_renders_desktop_snapshot`; **review the new `.snap` to confirm it is a
   full-area ░ fill, then accept it** (`cargo insta accept` or update the
   inline). All other program.rs tests insert probes into the *root* group
   (`program.group_mut()`), unaffected by this change — they must still pass
   untouched.

## Tests (this is the verification — D11)

Add a `#[cfg(test)]` module in `desktop.rs`. Cover:

1. **Ctor inserts background + records its id** (the behavioral test the snapshot
   can't give you): `Desktop::new(bounds, Desktop::init_background)` →
   `desktop.background()` is `Some(id)`, the group has exactly 1 child, and that
   child draws ░. (You can assert the child count via a small test hook or by
   rendering.)
2. **growMode** = `gfGrowHiX | gfGrowHiY` on the desktop's state
   (`desktop.state().grow_mode.hi_x && .hi_y`, others clear).
3. **`init_background` fill char is ░ (U+2591)** — construct via `init_background`,
   render on a `HeadlessBackend`, assert a cell symbol == `"\u{2591}"`. Guards the
   faithfulness bug directly (a snapshot alone would silently bake whatever you
   pick).
4. **No-background factory:** `Desktop::new(b, |_| None)` → `background()` is
   `None`, group is empty, `draw` is a no-op (doesn't panic).
5. **Mandatory snapshot** (Appendix B step 4): build a `Desktop` via
   `init_background` on a `HeadlessBackend`, `render` through `&mut dyn View`,
   `insta::assert_snapshot!` — a full-area ░ fill.
6. **Resize delegates:** `View::change_bounds` grows the desktop; assert the
   background child (gfGrowHiX/HiY) grew with it (proves delegation to the group's
   `change_bounds` works).

Follow the existing `background.rs` / `program.rs` test style (DrawCtx/Renderer
harness, `with_ctx` for a throwaway `Context` if you need one).

## Definition of done (run these; all must pass)

- `cargo test` — all green (incl. the migrated program.rs snapshot).
- `cargo clippy --all-targets -- -D warnings` — clean (the `background` field is
  kept live by the accessor + ctor test; no `#[allow(dead_code)]`).
- `cargo fmt --check` — clean.
- New `Desktop` snapshot reviewed and committed; `pump_renders_desktop_snapshot`
  re-accepted to ░ and reviewed.

## Deviations in play (apply mechanically; do not re-decide)

- **D2** embed-and-delegate: `Desktop` embeds `Group`, delegates the `View` trait.
- **D3** owner-data-down: no owner back-pointer; `background` is a local `ViewId`.
- **D7** `Role::Background` styles the fill (already handled inside `Background`).
- **D8** whole-tree redraw (no occlusion/`shutDown` redraw bracket).
- **D9** the cmNext/cmPrev modal/Z-reorder behavior defers to row 33.
- **D12** streamables dropped.

Report status as DONE / DONE_WITH_CONCERNS / NEEDS_CONTEXT / BLOCKED with a short
summary of what you built and the test/clippy/fmt results.
