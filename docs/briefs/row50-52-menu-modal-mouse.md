# Brief — Menu MODAL layer **Step-2 stage 2: MOUSE** (rows 50–52)

> Status: **DESIGN SETTLED (advisor-vetted).** Stage 1 (keyboard nav) landed
> (`ed0abfa`); the `MenuSession` capture handler, the re-apply loop, and
> `Deferred::OpenMenuBox`/`SetMenuCurrent`/`ctx.put_event` are all in place. This
> brief adds the **mouse arms** of the flattened `TMenuView::execute()` +
> `trackMouse`/`mouseInOwner`/`mouseInMenus` + the `evMouseDown` activation in
> `menu_view::handle_event`. C++ source of truth:
> `magiblot-tvision/source/tvision/tmnuview.cpp` (`execute` mouse arms lines
> 201–276, `trackMouse` 97–109, `mouseInOwner` 148–158, `mouseInMenus` 160–166,
> `do_a_select` 505–516, `handleEvent` evMouseDown 522–524).
>
> **Do NOT relitigate the architecture** — one `MenuSession` capture handler owns
> the whole open stack, settled in `row50-52-menu-modal.md` and proven by stage 1.
> This is purely the mouse-arm fill on the existing substrate. Read
> `src/menu/menu_session.rs` first (you are extending it, not rewriting it).

---

## 0. The model you are extending (already built — do not change its shape)

`MenuSession { levels: Vec<MenuLevel>, owner_size: Point }`. The **top** level is
the active C++ `execute()` frame; lower levels are suspended parents. The keyboard
path is `handle_key` → a `loop` that: steps the top level (`step_keyboard` →
`(action, cleared)`), pushes the new highlight via `ctx.request_set_menu_current`,
runs the **post-switch open-gate** (open a submenu / select a command), and on a
non-cleared `doReturn` **pops the top box and re-applies the SAME event to the new
top** (the flattening of C++'s `putEvent(e)`→parent-`getEvent`).

You will add the **mouse** counterpart sharing that exact loop tail. The cleanest
structure: factor the shared tail (set-current → reset-lastTarget → open-gate →
pending-command → doReturn-pop/re-apply) into one helper the keyboard and mouse
steps both feed, OR add a parallel `handle_mouse` loop that duplicates the tail.
**Prefer factoring** — a single `run(ev, ctx)` loop that calls `step_keyboard` or
`step_mouse` by event kind, then the shared tail — so the two paths cannot drift.
Stage 1's `handle_key` becomes that shared loop.

### Coordinate model (THE thing to get right)

All `position`s on `MouseEvent` and all `MenuLevel::bounds` are in the **root
group frame** (absolute; the root is at `(0,0)`). The session sees events via the
capture stack **before** view-tree translation, so positions are already
root-frame — no `makeLocal` conversion of the incoming event is needed.

C++ `getItemRect` returns **view-local** coords; the session's existing
`item_rect_local(index)` mirrors it. To hit-test against a root-frame mouse
position, offset by the level's origin:

```
item_rect_global(level, idx) = item_rect_local(idx) translated by level.bounds.a
```

(`Rect` has no translate helper? add a local `fn shift(r, p)` or inline
`a.x+p.x` … — match the existing helper style in the module.) The menu **bar** is
always inserted at `(0,0,w,1)`, so for the bar local == global; do the offset
anyway (uniform, and a box's origin is non-zero).

`mouse_in_view(level, pos)   = level.bounds.contains(pos)`
`mouse_in_owner(pos)`        = the PARENT level's current-item global rect contains
                              `pos` (C++ `parentMenu->getItemRect(parentMenu->current)`;
                              `parentMenu == 0` → `false`). The parent is
                              `levels[len-2]`; its `current` is `Some(i)` (you only
                              open a submenu from a named item).
`mouse_in_menus(pos)`        = ANY **parent** level (every level except the top)
                              has `bounds.contains(pos)` (C++ walks `parentMenu`
                              chain, excludes `this`).

---

## 1. Per-level loop-locals to ADD to `MenuLevel`

Stage 1 already has `auto_select: bool`. Add the other three C++ `execute()`
per-frame locals (each is **per level**, C++ re-inits them at every `execute()`
entry, so they never leak across levels):

- `last_target_item: Option<usize>` — C++ `lastTargetItem` (init `0`/`None`). The
  item whose submenu was most recently opened **from this level**. Drives the
  "click an open title to close it" behaviour. **Crux — see §3.1.**
- `mouse_active: bool` — C++ `mouseActive` (init `False`). Set `True` by
  `track_mouse` when the mouse lands on an item; **monotonic — never reset to
  `False` within a level's lifetime.**
- `first_event: bool` — C++ `firstEvent` (init `True`). True only while the level
  has not yet finished processing its first event (including the re-applied
  triggering event after an open). Guards exactly one thing (evMouseDown
  `!firstEvent && mouseInOwner → doReturn`). **See §3.4 for exactly when it flips.**

`itemShown` is **not** needed — D8 whole-tree redraw makes the C++
`if(itemShown!=current) drawView()` moot; the existing `request_set_menu_current`
each iteration already publishes the highlight.

Init at level construction: bar level in `activate` and child levels in
`open_submenu` both get `last_target_item: None, mouse_active: false,
first_event: true` (plus the existing `auto_select`).

---

## 2. `track_mouse` (C++ `tmnuview.cpp:97`)

```
void TMenuView::trackMouse(e, &mouseActive) {
    mouse = makeLocal(e.mouse.where);
    for(current = menu->items; current != 0; current = current->next)
        if(getItemRect(current).contains(mouse)) { mouseActive = True; return; }
    // falls out with current == 0 if nothing hit
}
```

Rust, on the **top** level (it always overwrites `current`):

```
fn track_mouse(&mut self, pos: Point) {
    let n = self.top().menu.items.len();
    for i in 0..n {
        if self.item_rect_global(self.top(), i).contains(pos) {
            self.top_mut().current = Some(i);
            self.top_mut().mouse_active = true;
            return;
        }
    }
    self.top_mut().current = None;   // C++ loop ends with current == 0
}
```

Note: separators **have** an item rect and CAN be hit (C++ iterates all items);
`current` can land on a separator (then the mouseUp/Down arms below treat
`name == 0` as "not a real target"). Do not skip separators in `track_mouse`.

---

## 3. The mouse step — `step_mouse(ev, ctx) -> (MenuAction, bool)`

Mirror `step_keyboard`'s `(action, cleared)` return. **No mouse arm ever calls
`clearEvent`** in C++ except via the open-gate/result tail, so `cleared` is
effectively `false` for every mouse `doReturn` that comes from a box (→ the
re-apply loop always carries a box's mouse `doReturn` up to its parent — exactly
the cross-level drag/close behaviour). Set `cleared = true` only where noted.

Dispatch by event variant. `is_bar = self.top().is_bar`; `pos = m.position`.

### evMouseDown (`tmnuview.cpp:201`)

```
if mouse_in_view(top, pos) || mouse_in_owner(pos) {
    track_mouse(pos);                          // sets top.current (maybe None), mouse_active
    if is_bar {
        top.auto_select = !current || lastTargetItem != current;   // see note
    } else if !top.first_event && mouse_in_owner(pos) {
        action = doReturn;                     // a box closes when you click its parent's title
    }
    // (action stays doNothing otherwise — the open-gate may still fire via auto_select)
} else {
    // Click outside this level's bounds and outside the parent item.
    // putClickEventOnExit is True for bar+box (only TMenuPopup=False, stage 3),
    // so the exit click is re-posted to the view tree — but ONLY at the bar level
    // (see §3.3); a box just returns and the re-apply loop carries the click up.
    action = doReturn;  exit_click = true;     // mark it; do not put_event here
}
```

`auto_select` note (C++ `size.y==1` only): `top.auto_select = (current.is_none()
|| top.last_target_item != top.current)`. With `current` freshly set by
`track_mouse`. This is what makes a bar click **open** the clicked title's box
(first click: `last_target_item==None != Some(title)` → True) yet **close** it on
the second click of the same title (after the box closed it set
`last_target_item == that title`, see §3.1 → equal → False).

### evMouseUp (`tmnuview.cpp:225`)

Always `track_mouse(pos)` first (no in-view gate). Then, in order:

```
track_mouse(pos);
if mouse_in_owner(pos) {
    top.current = top.menu.default;            // released on parent item → reset to deflt
    // action stays doNothing
} else if let Some(cur) = top.current {
    if top.menu.items[cur] is named (not Separator) {
        if Some(cur) != top.last_target_item        { action = doSelect; }
        else if is_bar                              { action = doReturn; }
        else { action = doNothing; top.last_target_item = None; }  // box: next up will open
    }
    // a separator (name==0): nothing — action stays doNothing
} else if top.mouse_active && !mouse_in_view(top, pos) {
    action = doReturn;                          // released outside after activating
} else if !is_bar {
    top.current = top.menu.default.or(Some(0));  // box margin/separator: highlight deflt/first
    // action doNothing
}
```

### evMouseMove (`tmnuview.cpp:262`) — only while a button is held

```
if m.buttons is any-down {
    track_mouse(pos);
    if !(mouse_in_view(top,pos) || mouse_in_owner(pos)) && mouse_in_menus(pos) {
        action = doReturn;                      // dragged off this box onto an ancestor menu
    } else if is_bar && top.mouse_active && Some-cur != top.last_target_item {
        top.auto_select = true;                 // drag to a new bar title → open it
    }
}
// buttons == 0 → no-op, action doNothing
```

`MouseAuto` and any other event: keep the stage-1 `Consumed` no-op (execute() has
no evMouseAuto arm).

### 3.1 CRUX — `last_target_item = current` on child-pop (advisor #1)

This is the **one new cross-level mechanism** stage 2 adds. C++ sets
`lastTargetItem = current; menu->deflt = current` in the open-gate block **after
`execView(child)` returns** — i.e. when a child box closes back to the level that
opened it. In the flattened loop that "execView returns" moment is the **pop** in
the doReturn branch. So at the pop point, BEFORE re-applying to the parent:

```
let closed = self.levels.pop();
ctx.request_close(closed.view_id);
let parent = self.top_mut();
if let Some(cur) = parent.current {
    parent.last_target_item = Some(cur);   // C++ lastTargetItem = current
    parent.menu.default     = Some(cur);   // C++ menu->deflt = current
}
parent.first_event = false;                // C++ firstEvent=False after execView returns (§3.4)
```

This already runs for the keyboard re-apply pops too (it is faithful there — the
keyboard arms simply never read `last_target_item`; `menu.default` update is the
correct C++ behaviour stage 1 happened to omit harmlessly). **Bite test:** drop
the `last_target_item` assignment and the "click open title closes it" test must
fail (the box reopens).

### 3.2 Post-switch reset `if last_target_item != current { last_target_item = None }` (advisor #3)

C++ runs this **every iteration, before the open-gate** (`tmnuview.cpp:357`). Add
it to the shared tail right after `request_set_menu_current`, on the **top** level:

```
if self.top().last_target_item != self.top().current {
    self.top_mut().last_target_item = None;
}
```

It is the "drag away then back reopens" half of the dance. Applies to both paths
(inert for keyboard).

### 3.3 Open-gate divergence: re-apply the triggering event into the child — mouse-down/move only (advisor #2)

The existing open-gate, on opening a submenu, returns `CaptureFlow::Consumed`. C++
`putEvent(e)` into the child's frame is gated `if((e.what & (evMouseDown |
evMouseMove)) != 0)`. So after `open_submenu`:

```
self.open_submenu(idx, submenu, ctx);
if matches!(ev, Event::MouseDown(_) | Event::MouseMove(_)) {
    continue;     // re-apply the SAME mouse event to the freshly-opened child (first_event=true guards it)
} else {
    return CaptureFlow::Consumed;   // keyboard + mouseUp: child opens and waits
}
```

The new child level has `first_event = true`, so the re-applied mouse-down's
evMouseDown arm does NOT instantly close it via the `!first_event && mouse_in_owner`
guard.

### 3.4 `first_event` lifecycle (advisor #4)

`first_event` is `true` until the level finishes processing one event:

- Level processes an event and **does not open a child and does not get popped**
  (doNothing / doSelect-command-end / doReturn-at-bar): set `top.first_event =
  false` at the end of that iteration (just before the `return`).
- Level **opens a child** (open-gate submenu): do NOT flip the parent's
  `first_event` now. It flips when the child pops back (§3.1 sets
  `parent.first_event = false`). This matches C++ `firstEvent=False` running after
  `execView` returns.

### 3.5 Click-outside-closes + re-post at the BAR (advisor, putClickEventOnExit)

A box's exit-click `doReturn` (the `else` branch of evMouseDown) re-applies up the
stack (cleared=false). Only when the **bar** (`levels.len()==1`) ends from an
exit-click do we re-post the mouse-down to the view tree so the view under it
recovers focus (C++ bar else-branch `putEvent(e)`; the bar's final-tail putEvent
does NOT fire because `parentMenu==0 && e.what!=evCommand`). So in the doReturn
branch, when `levels.len()==1` and the step set `exit_click`:

```
let r = self.end_session_with(None, ctx);
ctx.put_event(ev_clone);        // the original mouse-down, root-frame coords
return r;
```

For a non-exit bar `doReturn` (e.g. mouseUp released outside, `mouse_active &&
!mouse_in_view`), end the session with **no** re-post. Thread the `exit_click`
flag out of `step_mouse` (e.g. widen the return to `(action, cleared, exit_click)`
or stash it on `self`), keeping keyboard's two-tuple intact via a small adapter.

---

## 4. Activation — `evMouseDown` on the idle bar (`menu_view::handle_event`)

Stage 1 left a breadcrumb `_ => {}` for the mouse-down activation. C++
`handleEvent` evMouseDown → `do_a_select(event)` = `putEvent(event);
execView(this)` — re-post the click, then enter `execute()`. Flattened:

```
Event::MouseDown(m) if mv.state.size.y == 1 && mv.state.id().is_some()
        && mv.state.get_bounds().contains(m.position) => {
    // Build the bar-only session with fresh locals (current = menu->deflt set by
    // execute()'s prologue; the re-posted click's evMouseDown arm trackMouses to
    // the clicked title and sets auto_select). Then re-post the click so the
    // session's first dispatch processes it.
    menu_session::activate_mouse(bar_id, mv.menu.clone(), mv.state.get_bounds(),
                                 ctx.owner_size(), m, ctx);
    ev.clear();
}
```

Add `menu_session::activate_mouse(bar_id, bar_menu, bar_bounds, owner_size,
mouse: MouseEvent, ctx)`:

- Build the bar `MenuLevel` with `current = bar_menu.default`, `auto_select:
  false`, `last_target_item: None, mouse_active: false, first_event: true`.
- `ctx.request_set_menu_current(bar_id, bar_menu.default)` (initial highlight).
- `ctx.push_capture(session)`.
- `ctx.put_event(Event::MouseDown(mouse))` — the re-posted click (root-frame: the
  bar is at `(0,0)`, so the bar-local position delivered to `handle_event` equals
  root-frame; document this). The pump applies `push_capture` after dispatch, then
  drains `out_events` next pump → the session (now on the stack) processes the
  click through its evMouseDown arm and opens the clicked title's box.

**Do not** pre-open a box in `activate_mouse` (unlike the alt-shortcut
`activate`): the re-posted click + the evMouseDown arm + the open-gate do it, which
is the faithful `do_a_select` flow and also yields correct `auto_select`/
`last_target_item` for the second-click-closes behaviour.

Gate strictly on `size.y == 1` and `bounds.contains(position)` so only an actual
click **on** the bar activates (a click elsewhere on the desktop must not).

> The keyboard `activate` (cmMenu / alt-shortcut) is untouched. Keep it.

---

## 5. The `handle` dispatch

Replace the stage-1 mouse no-op arm. `Event::MouseDown | MouseUp | MouseMove` →
the shared mouse loop (`self.run(ev, ctx)` or `self.handle_mouse(...)`).
`Event::MouseAuto(_)` and the `_` catch-all stay `Consumed` (modal: nothing
beneath the session sees pointer events). The existing `Command(MENU)`,
`Command(other)`, and `Broadcast` arms are unchanged.

---

## 6. Verification — `pump_once` integration tests (the real proof)

Extend the stage-1 suite in `src/app/program.rs` (the `program_with_menu_bar` /
`modal_menu` / `top_box_current` / `bar_current` harness — reuse it). Bar bounds
are `(0,0,w,1)`; File title starts at x≈1, Edit after it. Compute click positions
from `item_rect_local` so they hit real items. Each test **bite-checked** (remove
the fix → it fails). Required, mapped to the advisor's list:

1. **click_bar_title_opens_box** — MouseDown on File (y=0) → File box opens
   (`group.len()==baseline+1`, `top_box_current==Some(Some(0))`). Drive the
   activation pump, then the re-posted-click pump.
2. **click_open_title_closes_box** (THE crux, bite §3.1) — open File via click,
   then MouseDown on File again → box closes (`group.len()==baseline`, bar still
   highlights File). Bite: drop the pop-time `last_target_item` assignment → box
   stays/reopens.
3. **drag_to_neighbour_title_reopens** — MouseDown File (open), MouseMove with
   button held onto Edit → File box closes, Edit box opens (auto_select via the
   evMouseMove bar arm + re-apply). Asserts the cross-level re-apply.
4. **click_outside_closes_and_reposts** (§3.5) — open File, MouseDown at a desktop
   point outside the bar and box → session pops (`capture_len==0`), and the
   mouse-down is re-posted to `out_events` (assert an `Event::MouseDown` survives).
   Bite: skip the `put_event` → no re-post.
5. **drag_into_submenu_keeps_open** (§3.3 mouse-down/move continue) — open File,
   MouseMove(button) onto the More submenu item → nested box opens
   (`group.len()==baseline+2`) without instantly closing (first_event guard). Or a
   MouseDown variant. Bite: return Consumed instead of `continue` after
   open_submenu for mouse → nested box never gets the carried event (highlight
   wrong / no nested open on the press).
6. **mouseup_on_command_posts** — open File, MouseUp on Open (idx 0; but note Open
   == default == last_target after open? choose an item where `current !=
   last_target_item` so doSelect fires — e.g. move to a different command first, or
   click-drag-release pattern) → the command is posted and the session closes.
   Pick the event sequence so the C++ `current != lastTargetItem → doSelect` arm is
   the one exercised; assert the posted command + `capture_len==0`.
7. **mouseup_on_box_margin_resets_to_default** (§ evMouseUp box-margin arm) — open
   File, MouseUp at a box interior point that is NOT on any item row (margin /
   separator) → `top_box_current` becomes the box default/first, session stays
   open. Bite: drop the `else if !is_bar` reset arm.

Add a snapshot only if a genuinely new *draw* state appears (none expected — the
draw layer already covers highlighted/selected). Use the stage-1
`INSTA_UPDATE=always` workflow if so.

Run: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D
warnings` (force a re-lint, a cached run can mask a fresh warning), `cargo fmt
--all --check`. Stage-1's 535 lib tests must still pass (the keyboard arms are
behaviour-preserved; the `menu.default`/`last_target_item`/`first_event` additions
must not perturb them — if a keyboard test breaks, the §3.1/§3.4 unification is
wrong, investigate, do not paper over).

---

## 7. Scope fence (do NOT do here)

- **Stage 3 — `TMenuPopup` (52)**: `menu->deflt=0`, `putClickEventOnExit=False`,
  the popup `execute`/`handleEvent` overrides + `popupMenu()` free fn. Separate
  brief. (Your `exit_click` re-post is unconditional-True here = the bar/box
  default; TMenuPopup will gate it.)
- **Wiring a real menu bar into `Program`** + the initial-regray gap + the
  `program_handle_event` modal-isolation breadcrumb → stage 4.
- **`cmTile`/`cmCascade` desktop geometry** → the row-32 breadcrumb, with stage 4.
- Streaming (D12), `getHelpCtx`, mouse auto-repeat/press-hold loops (no `MouseAuto`
  arm in execute()).

Keep the doc comments faithful (cite the `tmnuview.cpp` line for each arm). Update
the module header's "What is deferred to stage 2 (mouse)" block to "implemented",
and leave a fresh stage-3 breadcrumb.
