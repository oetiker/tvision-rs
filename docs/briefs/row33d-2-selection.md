# Implementer brief — Row 33d-2: window selection (cmNext/cmPrev + Alt-N + numbered windows)

You are porting magiblot/tvision behavior to idiomatic Rust in the `tvision`
crate (house alias `tv::`). **Row 33 (`TWindow`) is staged.** Stages **33a**
(Group/Context primitives), **33b** (TWindow core), **33c** (zoom), the
**substrate realignment** (global `ViewId` + `find_mut`/`remove_descendant`),
**Phase A** (`Event::Broadcast { command, source }`), and **33d-1** (drag / close
/ setState `{cmClose, cmZoom}`) are **all committed** — build on them. This is
stage **33d-2**, the *selection* half of `TWindow`: window navigation by
`cmNext`/`cmPrev`, selection by `Alt+digit` (`cmSelectWindowNum`), and the
`View::number()` plumbing they need. Port **faithfully**; the only departures are
the pre-decided deviations named below. Do **not** invent features.

This is a single **FOUNDATION** stage on the main thread — **no worktree**. It
touches: `src/view/view.rs` (trait), `src/view/group.rs`, `src/desktop/desktop.rs`,
`src/window/window.rs`, `src/app/program.rs`.

## Mental model — the select-vs-focus crux (read this first)

The C++ for cmNext/cmPrev (`TGroup::selectNext` → `findNext` + **`select()`**)
and Alt-N (the window's **`select()`**) call `select()`, **not** `focus()`. Our
Rust `Group` has **no standalone `select_child`** — only `focus_child` (which is
C++ `select()` **plus** an outgoing `valid(cmReleasedFocus)` guard) and the raw
`set_current`. **Use `focus_child`** (via `focus_next` / a new `focus_by_number`).
This is faithful, because:

- Both call sites are **already gated** on `valid(cmReleasedFocus)` upstream
  (cmNext/cmPrev via the desktop's own `valid` check; Alt-N via `canMoveFocus()`),
  so `focus_child`'s extra outgoing-validation re-check is **redundant and always
  passes** — it cannot refuse focus that the upstream gate already permitted.
- Windows carry **`ofTopSelect`** (`window.rs:145` `top_select = true`), so
  `focus_child` → `make_first` **raises** the window — exactly what C++ `select()`
  → `makeFirst` does. The two are behaviorally identical here.

State this reasoning in a code comment at each call site; a spec reviewer
comparing to the C++ will otherwise stop on "why `focus_child` and not `select`?".

## Explicitly DEFERRED — do NOT build, NO dead stubs

- **`cmResize` keyboard resize sub-mode** (arrows-until-Enter/Esc) → still
  deferred (no menu can trigger it; per 33c's "enable only commands whose handlers
  exist" principle we must not enable it). **Do NOT add `cmResize` to the window's
  setState enable set.** Leave the `TODO(33d-2/later, D9)` breadcrumb in
  `window.rs`.
- **Scrollbar auto-repeat / thumb-drag** (`scrollbar.rs` `TODO(row 31, D9)`),
  **close press-and-hold confirm** (`frame.rs` `TODO(row 33, D9)`), **modal
  teardown** (`exec_view`/the `ModalFrame` pop → row 34), **sibling tee-walk**,
  multi-scheme theming, shadow casting — all unchanged, untouched.

Building any of these half-wired is worse than a clean defer.

## C++ source of truth (read these before writing)

- `source/tvision/tdesktop.cpp` `TDeskTop::handleEvent` — the cmNext/cmPrev switch.
- `source/tvision/tprogram.cpp` `TProgram::handleEvent` (Alt-N block) +
  `TProgram::canMoveFocus`.
- `source/tvision/twindow.cpp` `TWindow::handleEvent` (the `cmSelectWindowNum`
  broadcast arm — we realize it as a **direct walk**, not a broadcast) +
  `TWindow::setState`.
- `source/tvision/tgroup.cpp` `TGroup::selectNext`.

The exact C++ bodies (already extracted) are reproduced inline below so you do not
have to re-derive them.

```cpp
// TDeskTop::handleEvent
TGroup::handleEvent( event );
if( event.what == evCommand ) {
    switch( event.message.command ) {
        case cmNext: if( valid(cmReleasedFocus) ) selectNext( False ); break;
        case cmPrev: if( valid(cmReleasedFocus) ) current->putInFrontOf( background ); break;
        default: return;                 // <-- NO clearEvent for other commands
    }
    clearEvent( event );                 // <-- reached ONLY for cmNext/cmPrev
}

// TGroup::selectNext
if( current != 0 ) { TView* p = findNext(forwards); if (p) p->select(); }

// TProgram::handleEvent (Alt-N block, runs BEFORE TGroup::handleEvent)
if( event.what == evKeyDown ) {
    char c = getAltChar( event.keyDown.keyCode );
    if( c >= '1' && c <= '9' ) {
        if( canMoveFocus() ) {
            if( message( deskTop, evBroadcast, cmSelectWindowNum, (void*)(size_t)(c-'0') ) != 0 )
                clearEvent( event );
        } else
            clearEvent( event );
    }
}
// TProgram::canMoveFocus  ->  return deskTop->valid(cmReleasedFocus);

// TWindow::handleEvent  (the cmSelectWindowNum broadcast arm)
else if( event.what == evBroadcast &&
         event.message.command == cmSelectWindowNum &&
         event.message.infoInt == number &&
         (options & ofSelectable) != 0 ) {
    select();
    clearEvent(event);
}

// TWindow::setState (the sfSelected arm)
TGroup::setState(aState, enable);
if( (aState & sfSelected) != 0 ) {
    setState(sfActive, enable);
    if( frame != 0 ) frame->setState(sfActive,enable);
    windowCommands += cmNext;                          // <-- UNCONDITIONAL
    windowCommands += cmPrev;                          // <-- UNCONDITIONAL
    if( (flags & (wfGrow | wfMove)) != 0 ) windowCommands += cmResize;  // (we DROP cmResize)
    if( (flags & wfClose) != 0 ) windowCommands += cmClose;
    if( (flags & wfZoom) != 0 ) windowCommands += cmZoom;
    if( enable ) enableCommands(windowCommands); else disableCommands(windowCommands);
}
```

## The six pieces

### Piece 1 — `View::number()` on the trait (drop Window's inherent getter)

In `src/view/view.rs`, add a defaulted trait method:

```rust
/// `TView`/`TWindow::number`. Base views are unnumbered. `Window` overrides.
fn number(&self) -> Option<i16> {
    None
}
```

In `src/window/window.rs`: **delete the inherent `pub fn number(&self) -> i16`**
getter (window.rs:196–199) and instead implement the trait method in `impl View
for Window`:

```rust
fn number(&self) -> Option<i16> {
    if self.number > 0 { Some(self.number) } else { None }
}
```

(The field stays named `number`.) The `Some` only when `> 0` mirrors that
`TWindow::number` defaults to `wnNoNumber` (0) for unnumbered windows; a window
numbered 0 is never an Alt-N target.

**Fix the one test** that asserts `w.number() == 3` (window.rs ~line 858): change
it to `View::number(&w) == Some(3)` (or add `use crate::view::View;` and call as a
trait method). Grep for other `.number()` call sites and update them.

### Piece 2 — `Group::focus_by_number`

In `src/view/group.rs`, add:

```rust
/// Select (raise + focus) the selectable child whose `number()` matches `num`.
/// Returns whether a match was found. Realizes the `cmSelectWindowNum` walk:
/// `focus_child` == C++ `select()` + a redundant-but-gated outgoing validation
/// (see the brief's select-vs-focus note); windows carry `ofTopSelect` so it
/// raises them, matching C++ `select()`.
pub fn focus_by_number(&mut self, num: i16, ctx: &mut Context) -> bool {
    // C++ cmSelectWindowNum arm gates on (options & ofSelectable).
    let target = self.children.iter().find_map(|c| {
        let s = c.view.state();
        if s.options.selectable && c.view.number() == Some(num) {
            Some(c.id)
        } else {
            None
        }
    });
    match target {
        Some(id) => {
            self.focus_child(id, ctx);
            true
        }
        None => false,
    }
}
```

**Note the explicit `ofSelectable` filter** — unlike cmNext (whose `find_next`
already filters selectable), the by-number path must check it itself (faithful to
the C++ arm's `(options & ofSelectable) != 0`).

### Piece 3 — `View::select_window_num` (trait tree-op)

In `src/view/view.rs`, add a defaulted trait method (consistent with the
`find_mut`/`remove_descendant` tree-op family):

```rust
/// Tree-op: ask this subtree to select the window numbered `num`. Returns
/// whether one matched. Default: no-op. `Desktop` overrides.
fn select_window_num(&mut self, num: i16, ctx: &mut Context) -> bool {
    let _ = (num, ctx);
    false
}
```

In `src/desktop/desktop.rs`, override it in `impl View for Desktop`:

```rust
fn select_window_num(&mut self, num: i16, ctx: &mut Context) -> bool {
    self.group.focus_by_number(num, ctx)
}
```

**Use the trait method — NOT an `as_any_mut` downcast** at the program call site
(keeps Program decoupled from the concrete `Desktop` type).

### Piece 4 — TDeskTop `cmNext`/`cmPrev`

In `src/desktop/desktop.rs`, replace the `handle_event` body (which is currently
just `self.group.handle_event(ev, ctx)` followed by the `TODO(row 33, D9)`
breadcrumb) with the faithful port:

```rust
fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
    self.group.handle_event(ev, ctx);
    if let Event::Command(cmd) = *ev {
        match cmd {
            Command::NEXT => {
                if self.group.valid(Command::RELEASED_FOCUS) {
                    // selectNext(False): findNext + select. focus_next == that
                    // (focus_child raises ofTopSelect windows; validation already
                    // gated by valid() above). `false` == C++ `forwards == False`.
                    self.group.focus_next(false, ctx);
                }
                ev.clear(); // cleared even when !valid (C++ break falls through)
            }
            Command::PREV => {
                if self.group.valid(Command::RELEASED_FOCUS) {
                    if let Some(cur) = self.group.current() {
                        // current->putInFrontOf(background): send current to the
                        // back, exposing the next window. NB: put_in_front_of's
                        // `target: None` means TO-TOP (the inverse); pass the
                        // resolved Some(background) so a future refactor can't
                        // silently flip cmPrev into a raise.
                        self.group.put_in_front_of(cur, self.background, ctx);
                    }
                }
                ev.clear();
            }
            _ => {} // C++ `default: return;` — no clearEvent for other commands
        }
    }
}
```

**Critical (advisor-flagged):**
- The `ev.clear()` for cmNext/cmPrev sits **outside** the `valid()` check — the
  C++ `if(valid(...))` guards only the *action*; the `break` always falls through
  to `clearEvent`. Clear even when `!valid`.
- Any **other** command → **no** clear (the C++ `default: return`).
- `self.background` is the existing `Option<ViewId>`; on a real desktop it is
  always `Some`, so the cmPrev guard `if let Some(cur)` is the only nullability
  concern (background passes through as-is).

Remove the `TODO(row 33, D9)` breadcrumb now that it is implemented.

### Piece 5 — Alt-N (`cmSelectWindowNum`) in `program_handle_event`

In `src/app/program.rs`, `program_handle_event` currently takes
`(group, ev, ctx, end_state)`. **Add a `desktop: Option<ViewId>` parameter** and
thread it from the call site (it is `Program::desktop`, accessible at the pump's
destructure). Then, **BEFORE** `group.handle_event(ev, ctx)** (faithful C++ order
— TProgram does the Alt-N block first), insert:

```rust
// Alt+digit window selection (cmSelectWindowNum). Faithful TProgram::handleEvent
// order: BEFORE the group dispatch. The window NUMBER is an integer, not a
// ViewId, so this is a DIRECT walk, not a Broadcast{source} (that substrate
// serves the polymorphic infoPtr subject case, not an int payload).
if let Event::KeyDown(k) = *ev
    && let Key::Char(c) = k.key
    && ('1'..='9').contains(&c)
    && k.modifiers.alt
    && !k.modifiers.ctrl
    && !k.modifiers.shift
{
    let num = (c as i16) - ('0' as i16);
    // canMoveFocus(): deskTop->valid(cmReleasedFocus) — desktop-specific, NOT
    // the root group's valid().
    let can = desktop
        .and_then(|id| group.find_mut(id))
        .map_or(false, |dt| dt.valid(Command::RELEASED_FOCUS));
    if can {
        let matched = desktop
            .and_then(|id| group.find_mut(id))
            .map_or(false, |dt| dt.select_window_num(num, ctx));
        if matched {
            ev.clear();
        }
        // can-but-no-match: leave the event LIVE — it falls through to
        // group.handle_event below (C++ message()==0 path: no clearEvent).
    } else {
        ev.clear(); // !canMoveFocus -> clearEvent (C++ else branch)
    }
}

group.handle_event(ev, ctx);
// ... existing cmQuit handling unchanged ...
```

**Critical (advisor-flagged) — the three-way clear matrix:**
1. `can && matched` → **clear**
2. `can && !matched` → **do NOT clear** (event stays live, reaches the group)
3. `!can` → **clear**

Confirm `Key` and `KeyModifiers` are imported in `program.rs` (add `use`s as
needed). Update the `program_handle_event` doc comment / remove the Alt-N
`TODO(33d)` breadcrumb.

### Piece 6 — `setState`: add `{cmNext, cmPrev}` (UNCONDITIONAL)

In `src/window/window.rs` `set_state` (the `StateFlag::Selected` arm, currently
toggling `{cmClose if wfClose, cmZoom if wfZoom}`):

**Add `cmNext` and `cmPrev` as UNCONDITIONAL enables** — the C++ does
`windowCommands += cmNext; += cmPrev;` with **no flag guard**, then guards
cmResize/cmClose/cmZoom on flags. So:

```rust
// cmNext/cmPrev: UNCONDITIONAL (C++ has no flag guard). cmResize stays DROPPED
// (no handler yet — 33d-2/later).
if enable {
    ctx.enable_command(Command::NEXT);
    ctx.enable_command(Command::PREV);
} else {
    ctx.disable_command(Command::NEXT);
    ctx.disable_command(Command::PREV);
}
toggle(Command::CLOSE, self.flags.close); // existing
toggle(Command::ZOOM, self.flags.zoom);   // existing
```

Do **not** route cmNext/cmPrev through the flag-gated `toggle` closure — they have
no flag condition. **Do NOT add `cmResize`** (it stays dropped per the deviation).

## Verification — the round-trip is what matters

Per the handover and advisor: the verification that proves this stage is the
**`pump_once` integration round-trip**, not a handler unit test in isolation (a
unit test exercises neither the command-enable filter nor the focused-event
routing that actually carries these commands to the desktop). Add:

**Integration tests (through `Program::pump_once`):**
1. **Alt-N selects a numbered window.** Build a `Program` with a desktop holding
   two selectable windows numbered 1 and 2; window 1 current. Inject
   `Alt+'2'` keydown; pump; assert window 2 is now the desktop's `current` (and
   raised — `ofTopSelect`). Assert the event was consumed.
2. **Alt-N with no matching number leaves the event live** (can-but-no-match):
   inject `Alt+'9'` with no window 9; assert `current` unchanged. (Optionally
   assert the keydown reached the group — e.g. nothing else consumed it.)
3. **cmNext cycles windows.** Two/three windows; select one; inject
   `Event::Command(Command::NEXT)`; pump; assert `current` advanced to the
   findNext target. (The command must be **enabled** — it is, because selecting a
   window ran `set_state(Selected)` → enabled `{cmNext, cmPrev}`. This is exactly
   the enable-filter path a unit test would skip.)
4. **cmPrev sends current to back / cycles Z-order.** Inject
   `Event::Command(Command::PREV)`; assert the Z-order changed (current moved
   behind background / a different window became reachable as next).

**Unit tests:**
5. `Group::focus_by_number` — matches a selectable numbered child, returns
   `false` for an absent number, skips a non-selectable child with that number.
6. `View::number` — base view `None`; `Window` with `number > 0` → `Some(n)`;
   `Window` with `number == 0` → `None`.

## Process / acceptance

- `cargo test` — all green (existing 282 + new).
- `cargo clippy --all-targets -- -D warnings` — clean.
- `cargo fmt --check` — clean.
- Add a snapshot test only if a visible change warrants it; this stage is mostly
  routing, so the round-trip assertions above are the core.
- Faithful to the C++ + the corrected guide; no extra features; deferred items
  left as clean grep-able breadcrumbs (do not delete the `cmResize`/scrollbar/
  frame/modal TODOs).
