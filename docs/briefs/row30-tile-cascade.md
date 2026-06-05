# Brief — `Desktop::tile`/`cascade` geometry + wire `cmTile`/`cmCascade` (Phase 4, FOUNDATION-ish)

**Goal (one change):** port `TDeskTop::tile`/`cascade` window-layout geometry
(`tdesktop.cpp`), expose them so the program command handler can drive them, wire
the long-standing `cmTile`/`cmCascade` breadcrumb in `program_handle_event`, and
make `examples/hello.rs` emit + demonstrate them. This closes the row-32
`TApplication` breadcrumb.

This is the lowest-numbered incomplete in-sequence Phase-4 work (per
`docs/HANDOVER.md` "NEXT"). It is FOUNDATION-ish (touches the `View` trait + the
delegate macro), so: full implementation + the standard two-stage review.

> **Port FROM the C++ verbatim.** Source of truth:
> `/home/oetiker/scratch/tvision-spec/magiblot-tvision/source/tvision/tdesktop.cpp`
> (full relevant text inlined below). Faithful by default (CLAUDE.md methodology);
> the only deviations are the ones called out here.

---

## The C++ to port (`tdesktop.cpp`, verbatim)

```cpp
inline Boolean Tileable( TView *p ) {
    return Boolean( (p->options & ofTileable) != 0 && (p->state & sfVisible) != 0 );
}

static short cascadeNum;
static TView *lastView;

void doCount( TView* p, void * ) {
    if( Tileable( p ) ) { cascadeNum++; lastView = p; }
}

void doCascade( TView* p, void *r ) {
    if( Tileable( p ) && cascadeNum >= 0 ) {
        TRect NR = *(TRect *)r;
        NR.a.x += cascadeNum;
        NR.a.y += cascadeNum;
        p->locate( NR );
        cascadeNum--;
    }
}

void TDeskTop::cascade( const TRect &r ) {
    TPoint min, max;
    cascadeNum = 0;
    forEach( doCount, 0 );
    if( cascadeNum > 0 ) {
        lastView->sizeLimits( min, max );
        if( (min.x > r.b.x - r.a.x - cascadeNum) ||
            (min.y > r.b.y - r.a.y - cascadeNum) )
            tileError();
        else {
            cascadeNum--;
            lock();
            forEach( doCascade, (void *)&r );
            unlock();
        }
    }
}

short iSqr( short i ) {
    short res1 = 2;
    short res2 = i/res1;
    while( abs( (int)(res1 - res2) ) > 1 ) {
        res1 = (res1 + res2)/2;
        res2 = i/res1;
    }
    return res1 < res2 ? res1 : res2;
}

void mostEqualDivisors(short n, short& x, short& y, Boolean favorY) {
    short i;
    i = iSqr( n );
    if( n % i != 0 )
        if( n % (i+1) == 0 )
            i++;
    if( i < (n/i) )
        i = n/i;
    if (favorY) { x = n/i; y = i; }
    else        { y = n/i; x = i; }
}

static short numCols, numRows, numTileable, leftOver, tileNum;

void doCountTileable( TView* p, void * ) {
    if( Tileable( p ) ) numTileable++;
}

int dividerLoc( int lo, int hi, int num, int pos) {
    return int(long(hi-lo)*pos/long(num)+lo);
}

TRect calcTileRect( short pos, const TRect &r ) {
    short x, y;
    TRect nRect;
    short d = (numCols - leftOver) * numRows;
    if( pos < d ) {
        x = pos / numRows;
        y = pos % numRows;
    } else {
        x = (pos-d)/(numRows+1) + (numCols-leftOver);
        y = (pos-d)%(numRows+1);
    }
    nRect.a.x = dividerLoc( r.a.x, r.b.x, numCols, x );
    nRect.b.x = dividerLoc( r.a.x, r.b.x, numCols, x+1 );
    if( pos >= d ) {
        nRect.a.y = dividerLoc(r.a.y, r.b.y, numRows+1, y);
        nRect.b.y = dividerLoc(r.a.y, r.b.y, numRows+1, y+1);
    } else {
        nRect.a.y = dividerLoc(r.a.y, r.b.y, numRows, y);
        nRect.b.y = dividerLoc(r.a.y, r.b.y, numRows, y+1);
    }
    return nRect;
}

void doTile( TView* p, void *lR ) {
    if( Tileable( p ) ) {
        TRect r = calcTileRect( tileNum, *(const TRect *)lR );
        p->locate(r);
        tileNum--;
    }
}

void TDeskTop::tile( const TRect& r ) {
    numTileable = 0;
    forEach( doCountTileable, 0 );
    if( numTileable > 0 ) {
        mostEqualDivisors( numTileable, numCols, numRows, Boolean( !tileColumnsFirst ));
        if( ( (r.b.x - r.a.x)/numCols == 0 ) || ( (r.b.y - r.a.y)/numRows == 0) )
            tileError();
        else {
            leftOver = numTileable % numCols;
            tileNum = numTileable - 1;
            lock();
            forEach( doTile, (void *)&r );
            unlock();
        }
    }
}

void TDeskTop::tileError() { }   // empty — a no-op hook
```

And `TView::locate` (`tview.cpp`), which `doTile`/`doCascade` call per child:

```cpp
void TView::locate( TRect& bounds ) {
    TPoint min, max;
    sizeLimits( min, max );
    bounds.b.x = bounds.a.x + range( bounds.b.x - bounds.a.x, min.x, max.x );
    bounds.b.y = bounds.a.y + range( bounds.b.y - bounds.a.y, min.y, max.y );
    if( bounds != getBounds() )
        changeBounds( bounds );   // + a drawView/shadow tail that is moot under D8
}
```

---

## Deviations / mapping (apply these mechanically)

### D-rule context
- **D8 whole-tree redraw + diff:** drop all `lock()`/`unlock()`/`drawView`/shadow
  tails. `change_bounds` mutating bounds is enough; the loop redraws.
- **`forEach` order:** C++ `forEach` visits `first()`→`last`. In our `Group`,
  `children[0]` == C++ `last` (bottom) and `children.last()` == C++ `first()` (top)
  — see the module doc in `src/view/group.rs`. **So `forEach` order ==
  `children.iter().rev()`.** This ordering is load-bearing: `tileNum`/`cascadeNum`
  decrement across the visit, so the *first-visited* (topmost) child gets the
  highest position/offset.
- `tileError()` is an empty no-op → port the C++ `if(error) tileError() else {...}`
  as **"if the error condition holds, do nothing (leave bounds unchanged); else lay
  out."** No panic, no error return.

### 1. `pub(crate) fn locate(view: &mut dyn View, mut bounds: Rect, owner_size: Point)` — a **free function** in `src/view/view.rs`

Put it next to the existing private `range` helper (`view.rs:516`). Body is the
faithful `TView::locate`:
```rust
pub(crate) fn locate(view: &mut dyn View, mut bounds: Rect, owner_size: Point) {
    let (min, max) = view.size_limits(owner_size);
    bounds.b.x = bounds.a.x + range(bounds.b.x - bounds.a.x, min.x, max.x);
    bounds.b.y = bounds.a.y + range(bounds.b.y - bounds.a.y, min.y, max.y);
    if bounds != view.state().get_bounds() {
        view.change_bounds(bounds);
    }
}
```
> **DO NOT make `locate` a `View` trait method.** That is a trap: the delegate
> macro would generate `fn locate(...) { self.group.locate(...) }` for `Window`,
> forwarding to the *group*, whose `size_limits` is 0×0 — bypassing `Window`'s
> 16×6 minimum (the exact hazard documented at `window.rs:787-791`). A free fn over
> `&mut dyn View` dispatches correctly with **zero** macro interaction: `Window`
> overrides `size_limits` in its `impl View` block (so virtual dispatch picks it up)
> and `change_bounds` forwards to the group (faithful `TGroup::changeBounds`).
>
> Leave `Window`'s existing inherent `locate` (`window.rs:330`, used by `zoom`)
> **untouched** — do not unify, do not remove. Out of scope.

`size_limits`/`change_bounds`/`state` are existing `View` trait methods; `range`,
`Rect`, `Point` are in scope. Confirm `view::locate` is reachable from
`src/desktop/desktop.rs` (same crate, `pub(crate)`).

### 2. `Group` seam — tileable children in forEach order

Add to `src/view/group.rs` (near `topmost_child_at`, the existing `firstThat`-style
helper):
```rust
/// Ids of tileable + visible direct children in C++ `forEach` order
/// (`first()`→`last` == `children` reversed). Backs `TDeskTop::tile`/`cascade`.
pub(crate) fn tileable_ids(&self) -> Vec<ViewId> {
    self.children
        .iter()
        .rev()
        .filter(|c| {
            let s = c.view.state();
            s.options.tileable && s.state.visible
        })
        .map(|c| c.id)
        .collect()
}
```
Field paths verified: `state().options.tileable` (`view.rs:132`),
`state().state.visible` (`view.rs:89`). Reuse the existing
`pub fn child_mut(&mut self, id) -> Option<&mut dyn View>` (`group.rs:168`) to reach
each child mutably for `view::locate`.

### 3. `View::tile` / `View::cascade` — defaulted no-op trait methods, **Desktop overrides**

Mirror `select_window_num` (`view.rs:760`, the established "program acts on the
desktop through `&mut dyn View` without a downcast" pattern). In `src/view/view.rs`:
```rust
/// `TApplication`-level `cmTile` → `deskTop->tile(getTileRect())`. Base: no-op
/// (only `TDeskTop` lays out windows). `r` is the desktop-local layout rect.
fn tile(&mut self, _r: Rect) {}
/// `TApplication`-level `cmCascade` → `deskTop->cascade(getTileRect())`. Base: no-op.
fn cascade(&mut self, _r: Rect) {}
```
**No `ctx` param** — `change_bounds` needs none and D8 handles redraw.

**Delegate-macro bookkeeping (mandatory, per CLAUDE.md):** add forwarders to
`tvision-macros/src/specs.rs` (alongside `select_window_num`):
```rust
("tile",    quote! { fn tile(&mut self, r: #k::Rect) { self.#f.tile(r) } }),
("cascade", quote! { fn cascade(&mut self, r: #k::Rect) { self.#f.cascade(r) } }),
```
Then bump the `delegate_view.rs` spy expected-count (currently **22 → 24**) and
add the two new methods to its spy trait-impl so the spy test still matches the
trait exactly. (Forwarding `tile`/`cascade` to an inner group is a harmless no-op
for non-desktop wrappers — the trap only applied to `locate`, which sizes *self*.)

### 4. `Desktop` — the geometry (`src/desktop/desktop.rs`)

- **Re-add the `tile_columns_first: bool` field** (default `false`) — the ctor
  comment at line 31 says it was dropped "because only `tile` reads it"; `tile`
  now reads it. C++ `tileColumnsFirst = False` in the ctor. `favorY = !tile_columns_first`.
- Implement the pure helpers as **free functions / private fns** in the module:
  `i_sqr(i: i32) -> i32`, `most_equal_divisors(n: i32, favor_y: bool) -> (i32, i32)`
  returning `(x, y)`, `divider_loc(lo, hi, num, pos) -> i32`, and
  `calc_tile_rect(pos, r, num_cols, num_rows, left_over) -> Rect`. The C++ uses file
  statics (`numCols`/`numRows`/`leftOver`) — pass them as params instead (no globals).
  - **`divider_loc` overflow:** C++ `int(long(hi-lo)*pos/long(num)+lo)`. Coords are
    `i32`; do the multiply in `i64`:
    `((hi - lo) as i64 * pos as i64 / num as i64) as i32 + lo`.
  - `i_sqr` uses `abs((int)(res1-res2)) > 1`; port faithfully with `i32`.
- Add the two override methods inside the `#[delegate]` `impl View for Desktop`
  block (the macro auto-skips methods you write — same as `handle_event` /
  `select_window_num`). Inside, operate on `self.group`:
  ```rust
  fn tile(&mut self, r: Rect) {
      let ids = self.group.tileable_ids();          // forEach order
      let n = ids.len() as i32;
      if n == 0 { return; }
      let favor_y = !self.tile_columns_first;
      let (num_cols, num_rows) = most_equal_divisors(n, favor_y);
      // tileError guard: skip layout if a cell would be zero-width/height.
      if (r.b.x - r.a.x) / num_cols == 0 || (r.b.y - r.a.y) / num_rows == 0 { return; }
      let left_over = n % num_cols;
      let owner_size = self.group.state().size;       // desktop size feeds child size_limits
      let mut tile_num = n - 1;                        // FIRST visited gets n-1
      for id in ids {
          let rect = calc_tile_rect(tile_num, r, num_cols, num_rows, left_over);
          if let Some(v) = self.group.child_mut(id) { view::locate(v, rect, owner_size); }
          tile_num -= 1;
      }
  }

  fn cascade(&mut self, r: Rect) {
      let ids = self.group.tileable_ids();            // forEach order
      let n = ids.len() as i32;                        // == doCount's cascadeNum
      if n == 0 { return; }
      let owner_size = self.group.state().size;
      // lastView = last tileable in forEach order; error check uses cascadeNum == n.
      if let Some(&last_id) = ids.last() {
          let (min, _max) = self.group.child_mut(last_id).unwrap().size_limits(owner_size);
          if min.x > r.b.x - r.a.x - n || min.y > r.b.y - r.a.y - n { return; } // tileError
      }
      let mut cascade_num = n - 1;                     // C++ decrements once before doCascade
      for id in ids {
          if cascade_num >= 0 {
              let mut nr = r;
              nr.a.x += cascade_num;
              nr.a.y += cascade_num;
              if let Some(v) = self.group.child_mut(id) { view::locate(v, nr, owner_size); }
              cascade_num -= 1;
          }
      }
  }
  ```
  **Pin the off-by-one (classic port bug):** `tile_num` starts at `n-1`;
  `cascade_num` starts at `n-1` (C++ counts up to `n` via `doCount`, checks the
  error with `cascadeNum == n`, *then* `cascadeNum--` → `n-1`, so offsets run
  `n-1 … 0`). The error check subtracts the **full count `n`**, not `n-1`.
- Update the module doc (lines 26-31): `tile`/`cascade`/`tileError` and
  `tile_columns_first` are no longer "deferred". Keep the doc faithful/brief.

### 5. Wire `program_handle_event` (`src/app/program.rs:1149`, the TODO)

Replace the `TODO(Phase 4: TApplication command handling)` breadcrumb with real
arms, after `group.handle_event` + beside the `QUIT` catch. Faithful
`TApplication::handleEvent`: `cmTile → deskTop->tile(getTileRect())`,
`cmCascade → deskTop->cascade(getTileRect())`, then `clearEvent`.
```rust
if let Event::Command(cmd) = *ev {
    if cmd == Command::TILE || cmd == Command::CASCADE {
        if let Some(id) = desktop {
            // getTileRect() = desktop child's local extent.
            let r = group.find_mut(id).map(|v| v.state().get_extent());
            if let (Some(r), Some(dt)) = (r, group.find_mut(id)) {
                if cmd == Command::TILE { dt.tile(r); } else { dt.cascade(r); }
            }
            ev.clear();   // mirror the QUIT catch — clearEvent after handling
        }
    }
}
```
(Match the existing borrow style — `get_tile_rect` is a `Program` method that can't
be reached here; compute the rect inline via the two `find_mut` calls, as the
Alt-N block above already does.) `Command::TILE`/`CASCADE` exist (`command.rs:114/116`)
and are enabled in `default_command_set`. Keep `cmDosShell` deferred (needs a
backend suspend seam) — leave a one-line breadcrumb.

### 6. `examples/hello.rs` — emit + demonstrate

- Set `tileable = true` on the 3 demo windows (C++ `TWindow` does **not** set
  `ofTileable`; the app does). Set it on each window's `state_mut().options.tileable`
  before/after `desktop.insert_view(...)` (use whatever public seam fits; if the
  window's state isn't reachable post-box, set it on the `Window` before boxing).
- Add **Tile** and **Cascade** items to the `Window` menu, wired to
  `Command::TILE` / `Command::CASCADE` (these now route). Update the example's
  "Known limitation" comment (Tile/Cascade are no longer un-routable).

---

## Verification (the standard — discriminating + bite-checked)

In `src/desktop/desktop.rs` tests (the module can reach `self.group` directly):
1. **tile lays N windows into `calc_tile_rect` cells.** Build a desktop (e.g.
   `0,0,80,24`) with 3 tileable windows; call `tile(get_extent())`; assert each
   window's resulting bounds equal the expected `calc_tile_rect(pos, …)` for its
   forEach position. Bite: a window placed at the wrong `tile_num` (off-by-one)
   must fail.
2. **non-tileable / invisible children are skipped** (insert one of each; their
   bounds stay unchanged; tileable ones still lay out).
3. **tileError guard leaves bounds unchanged** — a too-small rect (cell width or
   height would be 0) → no window moves.
4. **cascade offsets are `n-1 … 0`** — assert the first-visited (topmost,
   `children.last()`) window's `a` == `r.a + (n-1)` and the last-visited == `r.a + 0`.
5. **cascade sub-minimum guard** — a desktop narrower than `min + n` leaves bounds
   unchanged (drive it with windows whose `size_limits` min is the window 16×6).
6. **One `pump_once` test in `program.rs`** posting `Command::TILE` (as a menu item
   would), asserting the desktop's windows relocated (the breadcrumb path end-to-end)
   + the event was cleared.

No new `.snap` is strictly required (geometry is asserted numerically), but a
tile snapshot through the real renderer is welcome if cheap.

## Commands (Cargo workspace; set the target dir)
```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test  --workspace
cargo clippy --workspace --all-targets -- -D warnings   # force a fresh lint
cargo fmt --all --check
cargo build --example hello
```
Write **one file at a time, verifying as you go** (subagent incremental-write
discipline). Report the final `cargo test`/`clippy`/`fmt` output verbatim.

## Scope fence (do NOT do)
- No `cmDosShell` (needs a backend suspend seam — separate).
- No unifying/removing `Window::locate` (inherent, used by zoom — leave it).
- No `tileColumnsFirst` user-facing setter beyond the field + ctor default unless a
  test needs the column-first path (then a minimal `#[cfg(test)]` setter is fine).
- No streaming (D12), no `shutDown`.
- Don't refactor unrelated files. (`git diff` will be reviewed whole.)
