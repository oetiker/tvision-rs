# Splitter resize unification — design

**Date:** 2026-06-14
**Status:** approved (ready for implementation plan)

## Problem

The `Splitter` widget grew its own bespoke keyboard-resize mode bound to **F6**
(`enter_reconfig` → Tab/arrows/Enter/Esc). Three problems:

1. **F6 collides with `cmNext`** ("next window") — the classic Turbo Vision
   meaning. The splitter should not own F6.
2. **Mouse drag does not change the divider color.** Only the keyboard-selected
   divider turns to `FrameDragging`; a mouse-dragged divider stays at rest color.
3. **Resting divider color does not match the window frame.** Dividers always use
   `FramePassive` (gray) while an *active* window frame uses `FrameActive`
   (bright). A splitter is always a window body in practice, so the mismatch is
   visible.

The splitter is **always used as a window body** (gallery, tvdemo, splitter
examples). That makes the window the natural owner of the resize entry point.

## Decisions (locked with the user)

- **Keyboard trigger:** reuse the existing window `Command::RESIZE` path — *not*
  F6, not a new dedicated key.
- **One unified mode, Tab cycles targets:** entering resize mode starts on the
  window frame; **Tab/Shift-Tab** cycle the active target
  `window → divider 0 → divider 1 → … → window`. Arrows move the active target;
  it glows in the dragging color; **Enter** commits, **Esc** cancels everything.
- **Mouse:** can drag **any movable (non-Locked) divider at any time**, including
  Hidden-style ones (which become visible while dragged).
- **Resting color:** dividers match the **owning window frame state** —
  active → `FrameActive`, inactive → `FramePassive`, being-moved → `FrameDragging`.
  Matching is by **color**, not line-weight: dividers stay single-line.

## Design

### 1. Divider color (`Splitter::draw_dividers`)

Replace the current "passive normally / double-line frame glyph when reconfig"
logic with frame-matching color driven by **propagated state**. The window
already propagates `StateFlag::Active` and `StateFlag::Dragging` to every body
child (`Window::set_state` → `Group::set_state`), so the splitter reads its own
`state().state.active`.

```
moving = self.dragging == Some(i) || self.reconfig == Some(i)
role   = if moving               -> Role::FrameDragging
         else if state.active    -> Role::FrameActive
         else                    -> Role::FramePassive
glyph  = single-line always (frame_v for Cols, frame_h for Rows)
```

- The `active → double-line (frame_v_d/frame_h_d)` behavior is **removed**.
- A moved divider (mouse OR keyboard) now glows exactly like the window frame
  during resize.
- **Palette:** keep the Blue frame-role family (`FrameActive`/`FramePassive`/
  `FrameDragging`) — what the demos use. Cyan/Gray windows would need palette
  threading into the splitter; **out of scope**, noted as a follow-up.

### 2. Mouse — any movable divider, always

In `handle_event`'s `MouseDown` arm, change the grab gate from

```rust
let allowed = (style.draggable_live() || self.reconfig.is_some())
    && style.movable_in_reconfig();
```

to simply **"movable" (not Locked)**. Hidden dividers then drag too; they paint
in `FrameDragging` while `self.dragging == Some(i)` (falls out of §1). Hit-test
(`divider_at`) already detects the 1-cell gap regardless of style.

### 3. Drop F6 and the inline reconfig keys

Remove the `Key::F(6)` entry and the in-`handle_event` reconfig key block
(Tab / arrows / Enter / Esc / Shift+Tab). `enter_reconfig` / `exit_reconfig` /
`first_movable_divider` / `step_selection` are removed or repurposed into the
session API (§6). The `reconfig: Option<usize>` field **stays** — repurposed as
"which divider is the active resize target," now set by the capture via the
broker.

### 4. Unified resize mode (window-owned entry, Tab cycles targets)

On `Command::RESIZE`, the window builds a **target list** and pushes one extended
modal capture instead of the plain `KeyboardResizeCapture`:

- `targets = [Window?] + [each movable divider in the body splitter tree]`.
  - The `Window` target is included iff `move || grow` (current behavior).
  - Divider targets come from downcasting the body to `Splitter` and calling
    `collect_resize_targets()` (§6), which recurses into nested splitters
    (covers tvdemo's cols-with-nested-rows). Each carries
    `(splitter_id, index, orientation)`.
  - If `!move && !grow` but movable dividers exist, the `Window` target is
    omitted and the mode still runs (divider-only resize). RESIZE enablement in
    `Window::set_state` is extended to `move || grow || has_movable_divider`.
  - If there are **no** targets at all, do nothing (no capture pushed).
- The capture holds `targets: Vec<Target>` + `current: usize` where
  `Target = Window | Divider { splitter_id, index, orientation }`. It stays
  **decoupled from `Splitter` internals** — only ids + orientation.
- **Tab / Shift+Tab:** advance/retreat `current` (wrapping). On switch, clear the
  old target's highlight and set the new one:
  - leaving/entering `Window` → toggle the window's `StateFlag::Dragging`.
  - leaving a divider → `SplitterDivider { SetActive(None) }`.
  - entering a divider → `SplitterDivider { SetActive(Some(index)) }`.
- **Arrows:** `Window` target → existing `apply_delta` (resize window). Divider
  target → map the arrow to a `±1` delta along the divider's `orientation`
  (Cols → Left/Right, Rows → Up/Down) and emit
  `SplitterDivider { Nudge { index, delta } }`.
- **Enter:** commit — clear window `Dragging`, emit `EndSession { commit: true }`
  for each splitter, pop.
- **Esc:** cancel — restore window `save_bounds`, clear window `Dragging`, emit
  `EndSession { commit: false }` for each splitter (restores saved weights), pop.

Implementation note: this can extend `KeyboardResizeCapture` in place (it already
owns the window-resize half) or be a renamed `ResizeCapture`. It lives in
`src/window/window.rs` and references splitters only by id, so no `Splitter`
dependency leaks into the capture.

### 5. New `Deferred` broker variant (D3 sibling-broker pattern)

The capture holds only `&mut Context`, so it brokers divider ops through the pump
exactly like the scroller↔scrollbar broker:

```rust
// src/view/context.rs — enum Deferred
SplitterDivider { splitter: ViewId, op: DividerOp }

enum DividerOp {
    SetActive(Option<usize>),
    Nudge { index: usize, delta: i32 },
    EndSession { commit: bool },
}
```

The pump (`src/app/program.rs`, alongside the existing `find_mut`/`as_any_mut`
broker arms) resolves `group.find_mut(splitter).as_any_mut()` → `Splitter` and
calls the matching method. Touches the **view-tree** deferred family (same as
`ChangeBounds`/`SetState`/the scroller ops), so insertion-order drain stays
order-equivalent: no single dispatch co-queues conflicting ops on the same
splitter.

`begin_resize_session` is called **synchronously** by the window when building
the capture (the window has `&mut` access to its body at that point), so it does
**not** need a deferred variant.

### 6. Splitter `pub(crate)` API for the broker

- `collect_resize_targets(&self) -> Vec<(ViewId, usize, Orientation)>` — this
  splitter's movable dividers (in axis order) plus a recursive walk into pane
  children that are splitters.
- `begin_resize_session(&mut self)` — snapshot `saved_weights` for all slots;
  `reconfig = None` (no auto-selected divider; the capture drives selection).
- `set_active_divider(&mut self, sel: Option<usize>)` — set `self.reconfig`
  (drives the `FrameDragging` highlight in §1).
- `nudge_divider(&mut self, index: usize, delta: i32)` — move the divider by
  `delta` along its axis (current axis pos + delta → `drag_divider_to`),
  then relayout.
- `end_resize_session(&mut self, commit: bool)` — if `!commit`, restore
  `saved_weights`; clear `reconfig`, clear `saved_weights`, relayout.

## Verification (D11 snapshot tests, HeadlessBackend)

- Resting divider color in an **active** vs **inactive** window
  (`FrameActive` vs `FramePassive`).
- Divider in `FrameDragging` during a **mouse drag** (`MouseDown` + `MouseMove`).
- Divider in `FrameDragging` during **keyboard** target-selection.
- **Tab cycling** window ↔ divider(s): window-frame `Dragging` on the window
  target; divider highlight on a divider target; correct hand-off on switch.
- **Esc** restores divider weights (and window bounds) to pre-mode state.
- **Hidden** divider becomes draggable by mouse and shows while dragged.
- Nested splitter: `collect_resize_targets` enumerates inner dividers.

## Out of scope / follow-ups

- Cyan/Gray window-palette divider color (needs palette threading into the
  splitter). Blue-family only for now.
- Larger keyboard steps for divider nudge (Ctrl+arrow) — can mirror the window's
  `±8/±4` if wanted later.
