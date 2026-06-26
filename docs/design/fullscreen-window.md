# Design note — frameless fullscreen windows (a chrome-less "app body" mode)

> Status: **DESIGN** (not yet landed; expert-reviewed, findings folded in). A
> modern-TUI extension *alongside* the faithful port (precedent: `RegexValidator`
> next to the picture-mask port). It reuses existing seams — `Window::zoom`'s
> saved-geometry model, the Frame push-down setters, and the post-dispatch
> `Deferred` channel — rather than inventing substrate.

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
as zoom: a deep child of the desktop asking for a bigger box. `Off → Desktop →
Screen` is a natural cycle.

## Triggers (locked decisions)

- **API** — `Window::set_fullscreen(Fullscreen)`. The foundation.
- **Command** — `Command::FULLSCREEN`, handled in `Window::handle_event` when the
  window is active, cycling `Off → Desktop → Screen → Off`. Lives in
  `src/command.rs` with `ZOOM`/`CLOSE` (the framework's shared command vocabulary;
  it is handled by `Window` exactly as those are — `command.rs:108–113` shows the
  `Command("tv.zoom")` shape).
- **No default key binding** — apps bind their own key to the command.
- **Exit is the app's responsibility** — no built-in Esc escape hatch. The
  framework exposes the state + the cycle command; the app provides an exit
  affordance (the `⋮` menu is the natural home). A chrome-less window has no
  visible close box by design, and we don't want to fight apps that use Esc for
  their own content.

## The one coordination primitive

Both modes route through **one** new `Deferred` variant rather than ad-hoc
per-effect ops. This is the central design decision (it resolves the
owner-size-ordering and ViewId-identity problems below):

```rust
// new arm on the existing `Deferred` enum (src/view/context.rs:66):
SetFullscreen { window: ViewId, mode: Fullscreen },
```

`Window::set_fullscreen` / the `Command::FULLSCREEN` arm just record the target
`mode` on the window and **emit `SetFullscreen { window: self_id, mode }`**. The
window knows its own `ViewId` (global-id rule, D3), so the variant carries an id
exactly like every other view-tree `Deferred` arm (`ChangeBounds(ViewId, Rect)`,
`SetState(ViewId, …)`, `Close(ViewId)`, `FocusById(ViewId)` …). **All layout
coordination happens in the pump**, where owner sizes are known *after* any
sibling resize and the singleton menubar/desktop are reachable.

### Why centralize in the pump instead of inline like `zoom()`

`zoom()` (`window.rs:509`) resizes inline via `self.locate(...)` because it is
purely window-local. Fullscreen is **not** window-local: `Screen` mode must reach
*across* the tree to the menubar and *up* to the desktop, both forbidden during
the `&mut` borrow-stack dispatch. Doing `Desktop` inline but `Screen` deferred
would be two code paths; worse, the naive "expand desktop, then re-fit window"
done in one dispatch pass reads a **stale `owner_size`** (the desktop hasn't grown
yet — the deferred drain runs after dispatch). Routing **both** modes through the
single pump-applied `SetFullscreen` op gives one code path and fits the window
only *after* the desktop has actually been re-bounded. The one-pump latency is how
every `Deferred` effect already works (capture push, close, focus-by-id …); a
fullscreen toggle does not need sub-frame latency.

## Pump apply: `SetFullscreen { window, mode }`

The pump already owns the root layout (it builds desktop → status-line → menubar
at `program.rs:518–526`) and tracks `menu_bar: Option<ViewId>` — but **currently
discards it** in the drain destructure (`program.rs:1979` binds `menu_bar: _`).
This op requires the pump to **re-bind `menu_bar` and also track the desktop's
`ViewId`** so it can resolve those singletons. The pump also gains loop-owned
state:

```rust
fullscreen_window: Option<ViewId>,   // the window currently in Desktop/Screen, if any
```

Applying `SetFullscreen { window, mode }`:

1. **Frame border:** push `set_border_visible(mode == Off)` to the window's frame
   (via the existing `frame_mut()` downcast, mirroring `set_zoomed` at
   `window.rs:525–532`).
2. **Menubar:** `set_collapsed(mode == Screen)` on the resolved menubar id (no-op
   if there is no menubar).
3. **Desktop bounds:** the desktop top is row 0 when `mode == Screen`, else row 1
   — the pump computes this from the menu-bar height it already knows (layout
   knowledge lives here, not leaked into the `Deferred` enum). Apply via the
   desktop's `change_bounds`.
4. **Window bounds:** *after* the desktop has its final size, fit the window to the
   desktop's full extent for `Desktop`/`Screen`, or restore `restore_bounds` for
   `Off`. Because steps 3 and 4 run in the same drain, the owner size in step 4 is
   correct.
5. **Track:** set `fullscreen_window = (mode != Off).then_some(window)`.

## `Window` state

```rust
pub enum Fullscreen { Off, Desktop, Screen }   // Off → (a) → (b)

// Window gains:
fullscreen: Fullscreen,
restore_bounds: Option<Rect>,   // pre-fullscreen bounds to return to
```

`restore_bounds` is deliberately **separate from `zoom_rect`** (`window.rs:139`,
used exclusively by `zoom()`): fullscreen and zoom are independent saved-geometry
slots, so toggling one never clobbers the other's restore point. `set_fullscreen`
saves `restore_bounds` on the `Off → !Off` edge and consumes it on the `!Off →
Off` edge.

## `Frame` changes

Mirrors the existing owner-data-down setters (`set_flags` / `set_palette` /
`set_zoomed`, `window.rs:305–330`):

```rust
border_visible: bool,           // default true
fn set_border_visible(&mut self, v: bool)
```

`Frame::draw` guards the **box-drawing + title + icons** block (`frame.rs`
~335–432) on `border_visible`. **Interior fill stays unconditional** (so a
frameless window still paints a solid background that content draws over), **but
its style must change**: today the middle-row fill uses the *frame border* role
(`frame.rs:347–364`), which would paint a frameless window as a solid
frame-coloured rectangle. In frameless mode the interior must fill with the
**window content-background** style instead. (The exact `Role` is pinned during
implementation against the window palette's background — it must not be the frame
border role.)

## `client_rect()` seam (content fills to edges — locked decision)

Add **inherent** `Window::client_rect()` → the frame-inset rect when bordered, the
**full bounds** when frameless. It is an inherent method on `Window`, **not** a
`View` trait method, so it needs **no `#[delegate]` forwarder** in
`tvision-rs-macros/src/specs.rs`. `standard_scroll_bar()` (`window.rs:478–497`,
which today hardcodes the frame inset) is rewritten to key off `client_rect()`, so
a frameless window's scrollbars/content reach the screen edge instead of leaving a
ghost one-cell margin — the "becomes the background" look.

## `MenuBar` collapse

`MenuBar` gains `collapsed: bool` + `set_collapsed`. Two seams, **both** required
(draw-transparency alone is not enough — this corrects an earlier oversimplifying
claim):

- **Draw:** when **collapsed and not active**, `draw` paints only `⋮` at the
  top-right cell (e.g. `(width-1, 0)`) and **skips the full-width space-fill**, so
  the fullscreen window shows through the rest of row 0.
- **Hit-test passthrough:** the menubar still geometrically owns all of row 0
  (`grow_mode.hi_x`, `menu_bar.rs:66`) and is top-of-z-order, so the Group routes
  every row-0 `MouseDown` to it. When collapsed, `handle_event` must **explicitly
  pass through** mouse events that do **not** land on the `⋮` cell (return them
  unconsumed) so the fullscreen window below receives them. A click on the `⋮`
  cell — or F10 / a menu hotkey — **activates** the bar.

While **active**, the bar renders the **full** bar and runs all existing menu
navigation unchanged, then re-collapses when the menu closes. So collapse is a
draw-time + mouse-routing state with **no change to menu *navigation* logic**. If
there is **no menubar**, `Screen` mode simply covers the top row with no `⋮`.

## Lifecycle & edge cases

- **Resize:** the pump re-asserts the fullscreen layout for `fullscreen_window` on
  every terminal resize (re-running the `SetFullscreen` apply for the tracked
  window). This is the authoritative re-fit — it does **not** rely on the window's
  `rel` grow-mode (`window.rs:189–192`), which only scales proportionally and can
  drift by rounding.
- **Close / removal:** the `Command::CLOSE` arm in `Window::handle_event`
  (`window.rs:1180–1195`) emits `SetFullscreen { window, mode: Off }` before
  `request_close`, restoring chrome. **But** a programmatic
  `group.remove_descendant(window_id, …)` bypasses the `CLOSE` handler. Mitigation:
  on each pump pass, if `fullscreen_window` is `Some(id)` and `find_mut(id)` no
  longer resolves, the pump auto-restores chrome (un-collapse menubar, restore
  desktop bounds) and clears the slot. This makes chrome-restore robust against
  every removal path, not just the `CLOSE` command.
- **Other windows / modal dialogs** float on top of a fullscreen window normally —
  a fullscreen window is just a non-modal background window. Mode (a) gives the
  "frameless app body with dialogs over it" look with no app-level mode.
- **Currency/focus:** a fullscreen window participates in desktop window cycling
  like any other; covering the `background` child is purely visual.
- **Zoom interplay:** `zoom_rect` and `restore_bounds` are independent; entering
  fullscreen neither consults nor mutates zoom state, and the frame's zoom icon is
  hidden anyway while frameless.

## Testing (D11 snapshots, `insta`)

Build on `HeadlessBackend`; eyeball whole snapshots, include non-zero-origin /
resize cases (per the snapshot-at-origin lesson):

1. **Mode a** — a frameless window filling the desktop: no border, content
   background (not frame-blue) shows, content reaches edges; a normal dialog floats
   on top with its frame.
2. **Mode b** — a `Screen` window covering row 0, `⋮` at the top-right, status line
   intact.
3. **Collapsed menubar passthrough** — a mouse click off the `⋮` cell reaches the
   fullscreen window below; a click on `⋮` (and F10) activates the full bar, which
   then re-collapses.
4. **Round-trip** — `Off → Screen → Off` returns to the exact `restore_bounds` with
   the border and menubar restored.
5. **Resize while fullscreen** — `Screen` window still fills row 0..bottom-1 after
   a terminal resize.
6. **Removal restore** — removing a `Screen` window via `remove_descendant`
   restores the menubar/desktop (the pump's vanish check).
7. **No-menubar app** in `Screen` mode — top row covered, no `⋮` drawn.

## What this is *not*

- Not a status-line change (only the menu is covered).
- Not window reparenting — the window stays a child of the desktop; the **desktop**
  grows to expose the menu row.
- Not a new `Context::new` parameter — it **adds one `Deferred` variant**
  (`SetFullscreen`), per the deferred-effects rule.
- Not an app-level singleton "main view" — it is a per-window property; the
  frameless-background look is just a window that launched in `Desktop`/`Screen`.
- Not a `View`-trait addition — `client_rect()` is inherent on `Window`, so the
  `#[delegate]` forwarder list is untouched.
