# Row 49 — `TMenuView` (FOUNDATION): the passive menu substrate + command-graying broker

**C++:** `tmnuview.cpp`, `smnuview.cpp`, decl in `menus.h`. **Module:** `src/menu/mod.rs`
(extends the row-46 data tree). **Builds on:** row 46 (`Menu`/`MenuItem`), the D3
`Context`/`Deferred` broker idiom (rows 27/28/41), the row-28 defaulted-callback
precedent (`View::apply_list_scroll`).

This is a **scoped** port of `TMenuView`. The C++ class mixes two layers; row 49 ports
**only the passive (non-modal) layer**. The interactive modal layer (`execute()` and
everything it drives) is deferred to rows 50–52 with the unbuilt D9 `OpenModal` path.

---

## 1. Scope fence — what row 49 IS and IS NOT

**IN (this row):**
1. **Command-graying broker** (the spine): a new `Deferred::UpdateMenu(ViewId)` +
   `Context::request_update_menu` + a defaulted `View::update_menu_commands(&mut self,
   cs: &CommandSet)` callback, applied by the pump. Ports `updateMenu`.
2. **Passive accelerator dispatch:** the `evKeyDown` branch of `handleEvent` that, for a
   key matching a menu item's `keyCode`, posts that item's command. Ports
   `hotKey`/`findHotKey`.
3. **The passive `handle_event`** wiring (`evBroadcast cmCommandSetChanged` → regray;
   `evKeyDown` → accelerator post), with the **activation branches breadcrumbed**
   (`evMouseDown`, `evCommand cmMenu`, alt-shortcut → "needs OpenModal + draw, rows 50–52").
4. `MenuViewState { state: ViewState, menu: Menu }` — the embed target later menu views
   build on.

**OUT (deferred — breadcrumb each in code, do NOT stub):**
- `execute()` — the nested modal `getEvent` loop. Maps to the D9 **view-triggered async
  modal** path (`Deferred::OpenModal` + posted completion `Command`), *designed but
  unbuilt* (guide D9 "exec_view — corrected"). → **rows 50–52** (`TMenuPopup` is tagged
  "spawns/execs popup (D9)").
- `trackMouse`, `trackKey`, `nextItem`, `prevItem` — modal navigation; only `execute()`
  calls them. Their spec is subtle (separator-skipping; `prevItem` implemented *via*
  `nextItem`) and only an `execute()` integration test validates it — porting now risks a
  silently-wrong spec with nothing to catch it. → with `execute()`.
- `findItem` / `findAltShortcut` — feed only `execute()` + activation. → with `execute()`.
- `do_a_select`, `newSubView`, `mouseInOwner`/`mouseInMenus`/`topMenu` — activation /
  modal plumbing. → rows 50–52.
- `getItemRect`, `draw`, `getPalette`/`cpMenuView` — drawing; overridden by `TMenuBar`
  (50) / `TMenuBox` (51). → those rows.
- `getHelpCtx` — needs `current`/`parentMenu` (menu-selection-aware help). → with `execute()`.
- `current` / `parentMenu` fields — consumed only by `execute()`/`trackMouse`/`getHelpCtx`.
  **Omit from `MenuViewState` now** (omit-until-consumer, the row-32/48 rule); rows 50–52
  add them. Document this in the struct doc.
- **No `MenuView` trait yet.** The row-28 `ListViewer` trait earns its keep only because
  `draw`/`get_text` are *polymorphic seams the base's own logic dispatches into*. Row 49's
  passive layer dispatches into **no** overridable virtual, so a trait would be dead
  scaffolding. Introduce the trait at row 50/51 when `execute()` needs polymorphic
  `getItemRect`/`draw`. Row 49 uses **free functions** over `&Menu` / `&mut Menu` /
  `&MenuViewState`.
- Streaming (`writeMenu`/`readMenu`/`build`) — D12.

---

## 2. The command-graying broker (the spine — get this exactly right)

C++ `updateMenu` walks the menu tree; for each **command** item it sets
`disabled = !commandEnabled(command)`, recursing into submenus; returns whether anything
changed (so `handleEvent` can `drawView`). Triggered by the `evBroadcast cmCommandSetChanged`
broadcast.

```cpp
// tmnuview.cpp
Boolean TMenuView::updateMenu( TMenu *menu ) {
    Boolean res = False;
    for( TMenuItem *p = menu->items; p != 0; p = p->next )
        if( p->name != 0 ) {                       // skip separators
            if( p->command == 0 )                  // submenu: recurse
                { if( updateMenu(p->subMenu) == True ) res = True; }
            else {
                Boolean commandState = commandEnabled(p->command);
                if( p->disabled == commandState )  // (disabled == enabled) => out of sync
                    { p->disabled = Boolean(!commandState); res = True; }
            }
        }
    return res;
}
void TMenuView::handleEvent( TEvent& event ) { /* ... */
    case evBroadcast:
        if( event.message.command == cmCommandSetChanged )
            if( updateMenu(menu) ) drawView();
}
```

**Why a broker, NOT a `Context` command-set read-accessor** (this overturns the HANDOVER
note that said "add a read accessor on `Context`"): the command set lives on `Program`.
The pump's **apply-phase** `Context` (`program.rs` ~727) is alive across a loop whose
`EnableCommand`/`DisableCommand` arms call `command_set.enable_cmd()` (`&mut`). A
`&CommandSet` stored on `Context` would alias that `&mut` → borrow conflict. (It would
also add a param to `Context::new` at *every* call site — not "additive".) The
project rule is "**a new deferred capability ADDS A VARIANT, not a `Context::new` param**"
— so broker it, exactly like `Deferred::SyncListViewer` + `View::apply_list_scroll`.

**Build:**
- `Deferred::UpdateMenu(ViewId)` in `src/view/context.rs` (view-tree family; doc it like the
  other view-tree variants — order-equivalent under the insertion-order drain).
- `Context::request_update_menu(&mut self, id: ViewId)` → pushes that variant.
- Defaulted **`View::update_menu_commands(&mut self, _cs: &CommandSet)` {}** in
  `src/view/view.rs` (no-op default; exact `apply_list_scroll` precedent — placed right
  after it). Add the MAINTENANCE forwarder to `tvision-macros/src/specs.rs`:
  ```rust
  ("update_menu_commands",
   quote! { fn update_menu_commands(&mut self, cs: &#k::CommandSet) { self.#f.update_menu_commands(cs) } }),
  ```
  (`#k` = `::tvision`; `CommandSet` is already exported from the crate root — `lib.rs:96`.)
- Pump apply arm in `program.rs` (disjoint borrows — `group` and `command_set` are
  separate destructured fields, **no `ctx` needed**, like `ChangeBounds`):
  ```rust
  Deferred::UpdateMenu(id) => {
      if let Some(v) = group.find_mut(id) { v.update_menu_commands(command_set); }
  }
  ```
- The concrete `update_menu_commands` override on the menu view calls the free fn
  `menu_view::update_menu_commands(&mut self.mv.menu, cs)` (port of `updateMenu`,
  recursive). **Drop the `Boolean` return** — under whole-tree-redraw (D8) the
  `if updateMenu drawView` is moot; the next pump repaints unconditionally. Document this.

**Trigger path** (passive `handle_event`):
`Event::Broadcast { command: COMMAND_SET_CHANGED, .. }` → `ctx.request_update_menu(self_id)`
where `self_id = mv.state.id()` (the view's own id, set on insert — `View::id()`).

> **evBroadcast mask is MOOT — do not port a gate.** C++ `TMenuView` sets
> `eventMask |= evBroadcast` to *opt in* to broadcasts. In our `Group::handle_event`
> broadcasts fan out to **every** child unconditionally (`group.rs` ~809; test
> `broadcast_reaches_all_children_including_disabled`). So the menu receives
> `cmCommandSetChanged` automatically. Document this as a deviation; add no mask.

---

## 3. Passive accelerator dispatch (`hotKey`)

C++ (`findHotKey` recurses submenus; `handleEvent` posts the matched command):
```cpp
TMenuItem *TMenuView::findHotKey( TMenuItem *p, TKey key ) {
    while( p != 0 ) {
        if( p->name != 0 ) {
            if( p->command == 0 ) {                                  // submenu: recurse
                TMenuItem *T; if( (T = findHotKey(p->subMenu->items, key)) != 0 ) return T;
            } else if( !p->disabled && p->keyCode != kbNoKey && p->keyCode == key )
                return p;                                            // enabled cmd item, key matches
        }
        p = p->next;
    }
    return 0;
}
// handleEvent / evKeyDown (passive, the accelerator path only):
TMenuItem *p = hotKey(event.keyDown);
if( p != 0 && commandEnabled(p->command) ) {
    event.what = evCommand; event.message.command = p->command;
    event.message.infoPtr = 0; putEvent(event); clearEvent(event);
}
```

**Port** as a free fn `menu_view::hot_key(menu: &Menu, key: KeyEvent) -> Option<Command>`
(recurses submenus, skips separators, skips `disabled` items, matches `key_code == Some(key)`;
`None == kbNoKey` never matches — already handled by `Some(_) == key`). `KeyEvent: Eq`, so
compare directly.

In passive `handle_event` `evKeyDown`: if `hot_key(menu, k)` is `Some(cmd)` → `ctx.post(cmd)`
+ clear the event.

> **The C++ `commandEnabled(p->command)` re-check needs no live command-set read here.**
> Two reasons it's safe to drop: (a) `findHotKey`'s `!p->disabled` filter already excludes
> disabled items, and the cached `disabled` flag is kept current by the §2 regray broker;
> (b) even if a stale-enabled command were posted, the pump's command boundary filter
> (`program.rs` ~687: `drop_disabled` clears an `Event::Command` whose cmd isn't in
> `command_set`) drops it. The only gap is a one-idle-cycle staleness window between a
> command-set change and the next `cmCommandSetChanged` regray — accept + **document** it.

---

## 4. Structure & module layout

In `src/menu/mod.rs`, alongside the row-46 tree:

```rust
use crate::command::{Command, CommandSet};
use crate::view::{Context, ViewState, ViewId, View, DrawCtx};
use crate::event::{Event, KeyEvent};

/// Runtime (view) state shared by the menu views (TMenuView data members).
/// `current`/`parentMenu` are deferred (only execute()/getHelpCtx consume them — rows 50-52).
pub struct MenuViewState {
    pub state: ViewState,
    pub menu: Menu,
}

// free fns (the shared substrate; no trait yet — see brief §1):
pub fn hot_key(menu: &Menu, key: KeyEvent) -> Option<Command> { ... }      // findHotKey
pub fn update_menu_commands(menu: &mut Menu, cs: &CommandSet) { ... }       // updateMenu (no bool)
pub fn handle_event(mv: &MenuViewState, ev: &mut Event, ctx: &mut Context) { ... } // passive layer
```

`handle_event` reads `mv.menu` + `mv.state.id()`, posts/requests via `ctx`; it does **not**
mutate the menu (regray is deferred through the broker). Activation branches: breadcrumb
only.

`MenuViewState` is `pub` and lives in the public `menu` module; export what's needed via
`lib.rs` (`Menu`/`MenuItem`/`MenuBuilder` already exported — add `MenuViewState`, and the
free fns if you keep them `pub`; a `menu_view` submodule is fine if cleaner).

---

## 5. Verification (no snapshot — nothing draws, like row 46)

Unit tests (in `menu/mod.rs`):
- `hot_key`: matches a top-level command item by key; **recurses into a submenu** and finds
  a nested item's accelerator; returns `None` for a disabled item (bite: flip `disabled`,
  assert the match disappears); returns `None` for a separator / no-key item.
- `update_menu_commands`: build a menu with an item whose command is disabled in a
  `CommandSet`; run it; assert that item's `disabled` flipped to `true` and an enabled
  item stayed `false`; **recurses** into a submenu. Bite-check: a wrong predicate
  (`disabled = commandEnabled`) fails.

Integration test (real `pump_once`, the broker end-to-end — this is the headline):
- A `#[cfg(test)]` concrete `MenuProbe` embedding `MenuViewState`, impl `View` with
  `handle_event` → `menu_view::handle_event`, and `update_menu_commands` →
  `menu_view::update_menu_commands` (the FakeList precedent: a *real* consumer, not a dead
  stub). Insert it into a `Program` (or a Group driven by a test pump). `disable_command(X)`
  → run pumps until idle so the `cmCommandSetChanged` broadcast fires → the broadcast
  reaches `MenuProbe` → it requests `UpdateMenu` → the pump applies → assert the menu item
  for `X` is now `disabled`. **Discriminating + bite-checked** (remove the broker arm or
  the request → the item stays enabled → test fails).
- (Optional) accelerator: queue a `KeyDown` matching an item's `key_code` into a pump, assert
  the item's `Command` is posted (and dropped if the command is disabled — the boundary filter).

Run: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`,
`cargo fmt --all --check` (set `CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target`).
`cargo expand` is not needed unless adopting `#[delegate]` at a site (none here).

---

## 6. Faithfulness checklist for the reviewer (check against C++ + guide, NOT this brief)
- `updateMenu` predicate ported exactly (recurse submenus; only command items; `disabled =
  !commandEnabled`); return-bool intentionally dropped (D8 whole-tree redraw) — documented.
- `findHotKey` ported exactly (skip separators; recurse submenus; `!disabled` + keyCode
  match); the dropped `commandEnabled` re-check justified (cached `disabled` + pump filter).
- Broker, not a `Context` accessor — borrow rationale present; forwarder added to
  `specs.rs`; defaulted no-op on `View`.
- evBroadcast mask correctly identified as moot (no gate added).
- All OUT items (§1) are breadcrumbed, none stubbed (the row-32/48 dead-stub trap).
