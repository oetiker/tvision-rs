# Design note — frameless fullscreen windows (a chrome-less "app body" mode)

> Status: **DESIGN** (not yet landed). A modern-TUI extension *alongside* the
> faithful port (precedent: `RegexValidator` next to the picture-mask port). It
> reuses existing seams — `Window::zoom`, the Frame push-down setters, and the
> post-dispatch deferred channel — rather than inventing substrate.

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

- **API** — `Window::set_fullscreen(Fullscreen)` (and the cycle is reachable
  programmatically). This is the foundation.
- **Command** — `Command::FULLSCREEN`, handled in `Window::handle_event` when the
  window is active, cycling `Off → Desktop → Screen → Off`.
- **No default key binding** — apps bind their own key to the command. The
  framework stays opinion-free here.
- **Exit is the app's responsibility** — there is no built-in Esc escape hatch.
  The framework exposes the state + the cycle command; the app provides an exit
  affordance (the `⋮` menu is the natural home for it). Rationale: a chrome-less
  window has no visible close box by design; forcing a framework-level Esc would
  fight apps that want Esc for their own content.

## Part 1 — per-window core (delivers mode *a*, self-contained)

Touches **only `Window` + `Frame`** plus the **existing** `ChangeBounds` deferred
op. No Program / Desktop / MenuBar changes.

### `Window` state

```rust
pub enum Fullscreen { Off, Desktop, Screen }   // Off → (a) → (b)

// Window gains:
fullscreen: Fullscreen,
restore_bounds: Option<Rect>,   // pre-fullscreen bounds to return to
```

`restore_bounds` is deliberately separate from `zoom_rect`: fullscreen and zoom
are independent saved-geometry slots, so toggling one never clobbers the other's
restore point.

### `Frame` gains a suppression bit

Mirrors the existing owner-data-down setters (`set_flags` / `set_palette` /
`set_zoomed`):

```rust
border_visible: bool,           // default true
fn set_border_visible(&mut self, v: bool)
```

`Frame::draw` guards the **box-drawing + title + icons** block (`frame.rs`
~335–432) on `border_visible`. The **interior space-fill stays unconditional**, so
a frameless window still paints a clean background that content draws over — and a
chrome-less window therefore has no title, close box, or zoom icon (the point).

### `client_rect()` seam (content fills to edges — locked decision)

Add `Window::client_rect()` → the frame-inset rect when bordered, the **full
bounds** when frameless. `standard_scroll_bar()` (`window.rs` ~477) and app
content key off it, so a frameless window's content reaches the screen edge
instead of leaving a ghost one-cell margin. This is the "becomes the background"
look.

### Transition (Desktop)

- **Enter:** save `restore_bounds`; push `set_border_visible(false)` to the frame;
  defer `ChangeBounds(self_id, owner_full_rect)` — the desktop's full extent, with
  `owner_size` already available via `Context::owner_size()`.
- **Exit:** push `set_border_visible(true)`; defer `ChangeBounds(self_id,
  restore_bounds)`.

Mode *a* is essentially `zoom()` with the border switched off.

## Part 2 — top-level coordination (adds mode *b*)

A Screen-fullscreen window must reach **across** the tree to the menubar and
**up** to the desktop — both forbidden inline during the `&mut` borrow-stack
dispatch. That is exactly what the post-dispatch deferred channel is for
(`docs/design/deferred-effects.md`): **add two variants** to the deferred op enum,
applied by the pump at root level (where it can `find_mut` the singleton menubar
and re-bound the desktop):

```rust
// new deferred ops:
MenuBarCollapsed(bool)      // pump: locate menubar, set_collapsed(v)
DesktopCoversMenu(bool)     // pump: change desktop bounds — a.y = 0 (cover) | 1 (restore)
```

### Enter Screen

1. The `Desktop`-mode steps (save bounds, hide border).
2. Defer `DesktopCoversMenu(true)` — desktop top moves to row 0.
3. Re-fit the window to the now-taller desktop (its owner grew upward; growMode
   `hi_y` tracks the bottom only, so the top must be re-asserted explicitly via a
   `ChangeBounds` to the new owner-full extent).
4. Defer `MenuBarCollapsed(true)`.

**Z-order already cooperates:** root insertion order is desktop → status line →
menubar (`program.rs` ~518–524), so the collapsed menubar renders **on top** of
the fullscreen window and its `⋮` stays clickable.

### Exit Screen

Reverse: `MenuBarCollapsed(false)`, `DesktopCoversMenu(false)` (top back to row 1),
restore the window to `restore_bounds`, show the border.

### `MenuBar` collapse

`MenuBar` gains `collapsed: bool` + `set_collapsed`. When **collapsed and not
active**, `draw` paints only `⋮` at the top-right cell (e.g. `(width-1, 0)`) and
leaves the rest of the row transparent — it **skips the full-width space-fill** so
the fullscreen window shows through. Hit-testing that cell, or F10 / a menu
hotkey, **activates** the bar; **while active it renders the full bar** and runs
all existing menu navigation unchanged. Collapse is therefore a pure draw-time
state with zero changes to menu logic. If there is **no menubar**, Screen mode
simply covers the top row with no `⋮`.

## Lifecycle & edge cases

- **Closing a Screen-fullscreen window** must restore chrome: the window emits the
  Exit-Screen deferred effects as part of its close path, so the menubar
  un-collapses and the desktop top returns to row 1.
- **App resize:** the desktop's growMode keeps it filling the screen; the
  fullscreen window re-asserts the owner-full extent on resize so it keeps
  covering its target area.
- **Other windows / modal dialogs** float on top of a fullscreen window normally —
  a fullscreen window is just a non-modal background window. Mode (a) gives the
  "frameless app body with dialogs over it" look without any app-level mode.
- **Zoom interplay:** `zoom_rect` and `restore_bounds` are independent; entering
  fullscreen does not consult or mutate the zoom state, and the frame's zoom icon
  is hidden anyway while frameless.

## Testing (D11 snapshots, `insta`)

Build on `HeadlessBackend`; eyeball whole snapshots, include non-zero-origin /
resize cases (per the snapshot-at-origin lesson):

1. **Mode a** — a frameless window filling the desktop: no border, background
   shows, content reaches edges; a normal dialog floats on top with its frame.
2. **Mode b** — a Screen window covering row 0, `⋮` at the top-right, status line
   intact.
3. **Menubar activated while collapsed** — full bar renders during navigation,
   then re-collapses to `⋮`.
4. **Round-trip** — `Off → Screen → Off` returns to the exact `restore_bounds`
   with the border and menubar restored.
5. **No-menubar app** in Screen mode — top row covered, no `⋮` drawn.

## What this is *not*

- Not a status-line change (only the menu is covered).
- Not window reparenting — the window stays a child of the desktop; the desktop
  grows to expose the menu row.
- Not a new `Context::new` parameter — it ADDS deferred-op variants, per the
  deferred-effects rule.
- Not an app-level singleton "main view" — it is a per-window property; the
  frameless-background look is just a window that launched in `Desktop`/`Screen`.
