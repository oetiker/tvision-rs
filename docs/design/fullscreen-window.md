# Design note — frameless fullscreen windows (a chrome-less "app body" mode)

> Status: **REVISION 2 (in progress)** — the first cut landed on
> `feat/fullscreen-window` (Fullscreen enum, frameless Frame, collapsible MenuBar,
> pump engine, command). Live testing exposed two coupling bugs (un-zoom of a
> fullscreen window → small-but-frameless; frameless content keeps a 1-cell margin
> because it never reflows). Revision 2 **decouples the primitives**. The section
> below supersedes the original design (kept further down for history/rationale).

---

## Revision 2 — orthogonal primitives (the current target)

The first cut coupled three things that should be independent: **border
visibility** was welded to **fullscreen mode**, while **window bounds** were owned
by zoom *and* fullscreen *and* drag with two separate restore slots. That produced
incoherent states (zoom a fullscreen window → small frameless window with a stale
fullscreen slot). Revision 2 makes each concern an independent primitive and lets
"fullscreen" be a thin convenience that composes them.

### The primitives (each independently settable)

1. **`Window` border** — `set_bordered(bool)` (public, default `true`),
   independent of zoom/fullscreen/drag. The frame draws iff bordered. Replaces the
   fullscreen-driven `border_visible`; `Fullscreen` no longer owns it.
2. **Maximize** — ONE maximized-bounds concept with a single saved restore-rect on
   `Window` (`restore_rect: Option<Rect>`, `None` = not maximized). Both the
   **zoom** command and **fullscreen-Desktop** go through this one maximize/restore,
   so they cannot desync. This unifies (and replaces) the old `zoom_rect` + the
   pump's per-slot `restore` — there is now exactly one restore slot. (Un-zoom bug
   class removed by construction.)
3. **`MenuBar` collapse** — `set_collapsed(bool)`, already an independent property;
   the kebab renders as **`[⋮]`** (bracketed, so it reads as a clickable affordance).
4. **Desktop covers menu** — pump re-bounds the desktop to row 0 (Screen only),
   unchanged from R1.

### Border toggle reflows content as a resize + origin shift (the key change)

Window content components **already react to a window resize** via their
`grow_mode` — Revision 2 does **not** change that and adds **no** content-pinning
API. Removing the border is modeled as exactly that resize, with one twist:

- The client area grows by the border thickness (1 cell per edge): deliver the
  size delta to each **content** child through the **existing** `grow_mode`
  resize path (`calc_bounds`) — components reflow exactly as on a normal resize.
- **Plus** translate each content child's origin by the inset delta — `(-1, -1)`
  on border-removal, `(+1, +1)` on border-add — so the freed top-left border cell
  is absorbed and the top-left stays the top-left.

So bordered content at interior `(1, 1, w-1, h-1)` becomes frameless `(0, 0, w, h)`
(origin `-1,-1`, size `+2,+2`), and a partially-filling child reflows per its own
`grow_mode` just like a resize. This runs **only on a border change** (the plain
resize path is untouched, so there is no double-apply).

**Scrollbars are chrome, not content.** A `ScrollBar` the window created via
`standard_scroll_bar` anchors to an edge, not the client origin, so it does **not**
get the content transform — the window re-derives its position from the
`client_rect` formula (Task-4 logic) on a border change. The window distinguishes
them because it **owns the scrollbar ids it minted** (no new app API, no guessing):
every other non-frame child is content.

### Fullscreen composes the primitives

`Command::FULLSCREEN` still cycles `Off → Desktop → Screen → Off` (no default key),
and `Window::set_fullscreen(mode)` stays as the high-level entry — but both are now
defined in terms of the primitives:

- **Desktop** = maximize (into the desktop) + `set_bordered(false)`.
- **Screen** = Desktop + pump: `MenuBar::set_collapsed(true)` + desktop covers row 0.
- **Off** = restore (un-maximize) + `set_bordered(true)` + uncollapse + desktop row 1.

`set_bordered`, `maximize`/`restore`, and `MenuBar::set_collapsed` are **public**,
so an app can build a frameless desktop-filling "app body" directly without the
cycle. The inline-vs-deferred split is unchanged: the border + content reflow are
window-local (inline in `set_bordered`); the cross-tree menubar/desktop work stays
in the pump via `Deferred::SetFullscreen` (which now carries the composed intent).

### What this fixes

- **Un-zoom bug:** zoom and fullscreen share one maximize/restore; border is
  independent, so no state can be small-yet-frameless-by-accident. Every
  combination is a coherent, intentional state.
- **Content margin bug:** content reflows to the client area on border toggle
  (the whole point of "becomes the background").

### Delta vs. the shipped R1 branch (what changes)

- `Frame::set_border_visible` → driven by `Window::set_bordered`, no longer by
  fullscreen mode.
- `Window`: add `set_bordered` + content/scrollbar reflow on border change; replace
  `zoom_rect` and the pump's `restore` with one `restore_rect` + `maximize`/`restore`;
  `zoom` command routes through maximize.
- Pump `apply_fullscreen`: recomposed onto the primitives; `FullscreenSlot` no
  longer stores `restore` (the window owns the one restore-rect) — it tracks only
  what the pump must (the window id + whether the desktop/menubar are in their
  Screen state) for resize re-fit and removal-restore.
- `MenuBar`: kebab string `⋮` → `[⋮]` (bounds become a 3-cell `[⋮]` at the
  top-right; collapse stays a bounds change so hit-routing still works).
- Demos: no `insert_client` needed — content reflows automatically.

---

## Original design (Revision 1 — superseded above, kept for rationale)

> The text below describes the first cut as landed. Where R1 and R2 differ, R2
> governs. R1 reused existing seams — `Window::zoom`'s saved-geometry model, the
> Frame push-down setters, and the post-dispatch `Deferred` channel.

## The idea

Modern TUIs often render their primary content with **no visible window frame**
(it reads as the background) and tuck the menu behind a `⋮` "kebab" affordance in
the top-right corner. We want a window to be drivable into one of three states:

- **`Off`** — a normal framed window (today's behaviour).
- **`Desktop`** (mode *a*) — the frame border disappears and the window fills the
  **desktop** area; the menubar and status line stay put.
- **`Screen`** (mode *b*) — as *Desktop*, but the window also covers the **menu
  row**, and the menubar (if any) collapses to a single `⋮` (U+22EE VERTICAL
  ELLIPSIS) at the top-right corner of the screen. The status line is untouched.

It is a **per-window property** (locked decision), so it rides on the same model
as zoom: a deep child of the desktop asking for a bigger box.

## Triggers (locked decisions)

- **API — the primary entry point.** `Window::set_fullscreen(Fullscreen)` sets the
  mode directly (e.g. straight to `Screen`). Apps use this.
- **Command — a convenience cycler.** `Command::FULLSCREEN`, handled in
  `Window::handle_event` when the window is active, cycles `Off → Desktop → Screen
  → Off` by reading the window's current `fullscreen` and calling `set_fullscreen`
  with the next state. Lives in `src/command.rs` with `ZOOM`/`CLOSE` (the shared
  command vocabulary; `command.rs:108` shows the `Command("tv.zoom")` shape). The
  cycle is a UX convenience — direct-to-mode is the API, not the command.
- **No default key binding** — apps bind their own key to the command.
- **Exit is the app's responsibility** — no built-in Esc escape hatch. A
  chrome-less window has no visible close box by design, and we don't want to fight
  apps that use Esc for their own content; the `⋮` menu is the natural exit home.

## Division of labour: what is inline vs. deferred

The transition splits along a hard architectural line discovered in review:

- **Inline in `Window::set_fullscreen` (it has `&mut self`):** push
  `set_border_visible(mode == Off)` to the frame via `self.group.child_mut(frame_id)`
  + downcast — the **only** path that reaches the Frame, identical to how
  `set_flags`/`set_palette`/`set_zoomed` push owner data (`window.rs:286–330`). Also
  set `self.fullscreen = mode`. **This must be inline:** `Window::as_any_mut` is
  delegate-generated to forward to the inner `Group` (`specs.rs:94–95`), so an
  external `find_mut(window_id).downcast_mut::<Window>()` returns `None` — the pump
  *cannot* reach the Frame or any `Window`-private field. Then emit the deferred op
  (below).
- **Deferred to the pump (cross-tree, needs post-resize sizes):** menubar
  collapse + bounds, desktop bounds, and the window re-fit. These touch siblings
  the borrow-stack forbids inline, and the window re-fit must happen *after* the
  desktop is re-bounded (else `owner_size` is stale). All of this is done through
  the **`View` trait** (`change_bounds` on `find_mut`) — **no downcast needed**, so
  the `as_any_mut` limitation doesn't bite.

## The one coordination primitive

```rust
// new arm on the existing `Deferred` enum (src/view/context.rs:66):
SetFullscreen { window: ViewId, mode: Fullscreen },
```

Carrying only the **window** id is consistent with `Deferred::UpdateMenu(ViewId)`
(`context.rs:197`), which likewise carries one id and lets the pump supply the
other participant from its own destructured state. The pump resolves the singleton
menubar/desktop from its own ids (see below).

## Loop-owned state (in `Program`/the pump)

```rust
fullscreen: Option<FullscreenSlot>,
struct FullscreenSlot { window: ViewId, mode: Fullscreen, restore: Rect, shadow: bool }
```

Putting `restore` **here, not on `Window`**, is deliberate: the pump captures the
window's pre-fullscreen bounds via `find_mut(window).state().get_bounds()` on the
`Off → !Off` edge, and re-applies it via `change_bounds` on the `!Off → Off` edge.
(`get_bounds` is a `ViewState` method, `view.rs:508` — hence `.state().get_bounds()`;
`change_bounds` *is* a `View` trait method, `view.rs:996`, so neither needs a
downcast.) The slot also captures the window's `shadow` flag verbatim (apps may
have cleared the `window.rs:185` default) so it is restored to its real prior value,
not hardcoded. So **`Window` needs no `restore_bounds` field at all** — only
`fullscreen: Fullscreen` so the cycler can read current state. (`zoom_rect` at
`window.rs:139` is untouched and independent.)

## Pump apply: `SetFullscreen { window, mode }`

Factored into a single function `apply_fullscreen(window, mode)` reused by both the
deferred drain **and** the resize arm (DRY — see Lifecycle). The pump already binds
`desktop: Option<ViewId>` in the drain destructure (`program.rs:1975`); it must
also **un-discard** `menu_bar` (currently `menu_bar: _` at `program.rs:1979`).
Sequential `find_mut` borrows in one arm are an established pattern
(`ClipboardEditorPaste`, `program.rs:2619–2633`). Steps:

1. **Edge bookkeeping:** if entering (`slot` was `None`/`Off`, `mode != Off`),
   capture `restore = find_mut(window).state().get_bounds()` and the window's
   `shadow` into the slot, and clear the window's shadow. If exiting, read
   `restore`/`shadow` from the slot for step 4 and restore the shadow. Re-apply
   while already fullscreen (resize) must **not** recapture `restore`/`shadow`.
2. **Menubar:** `set_collapsed(mode == Screen)` **and** `change_bounds` it — full
   top row when not collapsed, the single `⋮` cell (`Rect::new(w-1, 0, w, 1)`) when
   collapsed. The bounds change is what makes hit-testing work (see MenuBar below).
   No-op if there is no menubar.
3. **Desktop bounds:** top = row 0 when `mode == Screen`, else row 1 — computed
   from the menu-bar height the pump already knows (layout knowledge stays in the
   pump, not in the `Deferred` enum). Apply via the desktop's `change_bounds`.
4. **Window bounds:** *after* the desktop has its final size — fit the window to the
   desktop's full extent for `Desktop`/`Screen`, or to `slot.restore` for `Off`.
   Same-drain ordering makes the owner size correct.
5. **Slot:** set `fullscreen = (mode != Off).then(|| FullscreenSlot { window, mode,
   restore })`.

(The frame border was already toggled inline in step 0, before the op was emitted.)

## `Window` changes

```rust
pub enum Fullscreen { Off, Desktop, Screen }   // closed set → enum (WindowPalette precedent, window.rs:87)

// Window gains exactly one field:
fullscreen: Fullscreen,
```

`set_fullscreen(mode)`: toggle frame `border_visible` (inline downcast), set
`self.fullscreen = mode`, emit `SetFullscreen { window: self.id(), mode }`. (The
shadow flag is captured/cleared/restored by the pump via the slot — see above — so
the prior value is preserved; `set_fullscreen` does not touch it.) The
`Command::FULLSCREEN` arm reads `self.fullscreen`, computes the next state, and
calls `set_fullscreen`.

**Window drag guard:** the row-0 / bottom-row drag-start in `handle_event`
(`window.rs:1275–1298`) must be **suppressed when `fullscreen != Off`** — otherwise
a frameless window starts a title-drag from content at row 0.

## `Frame` changes

```rust
border_visible: bool,           // default true; pushed down like set_zoomed
fn set_border_visible(&mut self, v: bool)
```

- **`draw`** keeps the **interior space-fill unconditional** but guards the
  **border edges + top/bottom rows + title + icons** on `border_visible`. This is
  **not** a single `if` wrapper: the middle-row loop (`frame.rs:351–360`)
  *interleaves* the interior fill with the left/right edge `put_char`s in one pass,
  so it must be **split** — an unconditional `for y { for x in 1..w-1 { fill } }`,
  and a `border_visible`-guarded pass for the `(0,y)`/`(w-1,y)` edges, top/bottom
  rows, and title/icons. The fill stays in its **existing role** (`frame.rs:352–355`):
  that `border` role *is* the window-body background for Blue/Cyan/Gray windows in
  both states — no new `Role` is needed (an earlier "swap to a content role" idea
  was a false alarm). (Whole-window content via `client_rect()` overdraws the fill;
  the fill just guarantees no desktop-background bleed-through in sparse windows.)
- **`handle_event`** (`frame.rs:458–514`) must guard its entire `MouseDown` arm on
  `border_visible`: when frameless, return immediately, arming **no** close/zoom
  capture. Otherwise the invisible close zone (cols 2–4, row 0) and zoom zone (cols
  w-5..w-3) silently fire `CLOSE`/`ZOOM` on a frameless window's content.

## `client_rect()` seam (content fills to edges — locked decision)

Add **inherent** `Window::client_rect()` → the frame-inset rect when bordered, the
**full bounds** when frameless. Inherent, **not** a `View` trait method, so it
needs **no `#[delegate]` forwarder** in `specs.rs`, and no `&dyn View` consumer
needs it (`standard_scroll_bar` is inherent same-impl, `window.rs:477`). That
method is rewritten to key off `client_rect()` so a frameless window's
scrollbars/content reach the screen edge — the "becomes the background" look.

## `MenuBar` collapse

`MenuBar` gains `collapsed: bool` + `set_collapsed`. Collapse is driven by the
**pump** via `set_collapsed` **plus a `change_bounds`** to the `⋮` cell (step 2
above) — **not** by draw-transparency. This is the key correction from review:
RSTV's `Group` has **no event bubbling** for positional events (`group.rs:25–28`,
`1452–1487`) — it hit-tests one topmost child and delivers once, so a "drawn
transparent but full-width" menubar would still swallow every row-0 click. By
**shrinking the menubar's bounds** to the `⋮` cell, the root group's hit-test
routes the rest of row 0 to the (expanded) desktop, and thus to the fullscreen
window, with no special passthrough logic.

- **Draw:** when collapsed, paint only `⋮` at `(w-1, 0)`.
- **Activation = a corner popup, NOT a width-reclaim.** The collapsed bar stays
  `⋮`-cell-sized *even while active*. `MenuBar` already hand-writes `handle_event`
  (`menu_bar.rs:151–153`), so it adds a `collapsed` guard at the top that, on
  activation (a `MouseDown` on the `⋮` cell, or the same `cmMenu`/alt-shortcut
  triggers the normal bar honors — keyboard events reach the bar regardless of
  width), calls the existing **`pub menu::popup_menu`** (re-exported at
  `menu/mod.rs:46`) anchored at `(w-1, 0)`. `auto_place_popup` (`menu_session.rs:1179`)
  right-aligns the box at the corner starting row 1 — a **vertical kebab menu** —
  and the session handles navigation/submenus/close normally. The non-collapsed
  path delegates to `menu_view::handle_event` as today.

  **Why not reclaim width** (the trickiest seam, now resolved): the menu session
  *freezes* the bar's bounds into `MenuLevel.bounds` at activation
  (`menu_session.rs:1065`) and derives dropdown origins from it
  (`menu_session.rs:823`); a deferred `change_bounds` updates `ViewState` but never
  the live session, so a "reclaim width" bar would compute a **zero-width** dropdown
  origin and silently render nothing. The popup path sidesteps this entirely and
  reuses existing machinery. One behaviour to document: `popup_menu` sets
  `put_click_event_on_exit = false` (`menu_session.rs:1163`) — a click outside the
  kebab closes it without re-posting, which is the right behaviour for a corner
  affordance.

If there is **no menubar**, `Screen` mode simply covers the top row with no `⋮`.

## Lifecycle & edge cases

- **Resize:** the terminal-resize arm (`program.rs:1995–2003`, which already
  applies layout **inline** via `change_bounds`) calls `apply_fullscreen(window,
  slot.mode)` for the tracked slot — the same function the drain uses (DRY).
  Tracking `mode` in the slot is required (the desktop top differs by mode).
  **Hard sequencing requirement:** `apply_fullscreen` must run **after** the root
  `group.change_bounds` cascade (`program.rs:2002`), because that cascade re-stretches
  the collapsed menubar back to full width via its `grow_mode.hi_x`
  (`menu_bar.rs:66`) — `apply_fullscreen` then re-shrinks it to the `⋮` cell. The
  re-apply does **not** recapture `restore`/`shadow`.
- **Close / removal:** the `Command::CLOSE` arm (`window.rs:1180–1195`) calls
  `set_fullscreen(Off)` before `request_close`, restoring chrome. **But** a
  programmatic `group.remove_descendant(window_id, …)` bypasses the `CLOSE`
  handler. Mitigation: each pump pass, if `fullscreen` is `Some(slot)` and
  `find_mut(slot.window)` no longer resolves, the pump auto-restores chrome
  (un-collapse + re-bound the menubar, restore desktop bounds) and clears the slot.
  Robust against every removal path.
- **Shadow:** `Window::new` defaults `shadow = true` (`window.rs:185`), but an app
  may have cleared it. The pump **captures the actual value** into the slot and
  clears it on `Off → !Off`, then restores that captured value verbatim on
  `!Off → Off` — never hardcoding `true`.
- **Other windows / modal dialogs** float on top of a fullscreen window normally —
  it is just a non-modal background window. Mode (a) gives the "frameless app body
  with dialogs over it" look with no app-level mode.
- **Currency/focus:** a fullscreen window participates in desktop window cycling
  like any other; covering the `background` child is purely visual.

## Testing (D11 snapshots, `insta`)

Build on `HeadlessBackend`; eyeball whole snapshots, include non-zero-origin /
resize cases (per the snapshot-at-origin lesson):

1. **Mode a** — frameless window filling the desktop: no border, the window-body
   background shows, content reaches edges; a normal dialog floats on top framed.
2. **Mode b** — a `Screen` window covering row 0, `⋮` at top-right, status line
   intact.
3. **Collapsed-menubar hit routing + corner popup** — a `MouseDown` off the `⋮`
   cell reaches the fullscreen window (because the menubar bounds are the `⋮` cell
   only); a click on `⋮` (and F10) opens a **kebab popup right-aligned at the
   corner** (the bar stays `⋮`-sized), and a click outside it closes it.
4. **Frameless hotspots are dead** — a click at the old close/zoom/title-drag
   coordinates does nothing (no `CLOSE`/`ZOOM`/drag).
5. **Round-trip** — `Off → Screen → Off` returns to the exact captured `restore`
   bounds, border + menubar + shadow restored.
6. **Resize while fullscreen** — `Screen` window still fills row 0..bottom-1 after
   a terminal resize; `restore` not clobbered.
7. **Removal restore** — removing a `Screen` window via `remove_descendant`
   restores menubar/desktop (the pump vanish check).
8. **No-menubar app** in `Screen` mode — top row covered, no `⋮`.

## What this is *not*

- Not a status-line change (only the menu is covered).
- Not window reparenting — the window stays a child of the desktop; the **desktop**
  grows to expose the menu row.
- Not a new `Context::new` parameter — it **adds one `Deferred` variant**
  (`SetFullscreen`), per the deferred-effects rule.
- Not a new `Role` — the existing frame interior-fill role is the window body.
- Not an app-level singleton "main view" — it is a per-window property.
- Not a `View`-trait addition — `client_rect()` is inherent; the `#[delegate]`
  forwarder list is untouched.
- Not draw-transparency passthrough — collapse is a real bounds change, because
  the `Group` does not bubble positional events.
