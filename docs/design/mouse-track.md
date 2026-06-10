# Design note — the MouseTrackCapture seam + the evMouseAuto synthesizer (backlog A3)

> Status: **LANDED** (this branch). The seam ships with one adopter (Button);
> the remaining widget adoptions (cluster, scrollbar, frame, list viewer,
> editor, menu, …) are the B2 fan-out batch.

## Baseline (the C++ mechanism)

**The hold loop.** `TView::mouseEvent` (tview.cpp:636-643) is a *blocking*
nested event loop:

```cpp
Boolean TView::mouseEvent(TEvent& event, ushort mask)
{
    do {
       getEvent(event);
        } while( !(event.what & (mask | evMouseUp)) );
    return Boolean(event.what != evMouseUp);
}
```

Every press-and-hold widget spins `do { body } while (mouseEvent(event, mask))`:
the body runs once for the press, then once per masked event (`evMouseMove`,
`evMouseMove|evMouseAuto`, or `evMouse`), and the loop exits on `evMouseUp` —
*discarding* every event outside `mask | evMouseUp` (keys, commands,
broadcasts: the hold is modal). Some callers re-read the up position after the
loop (tcluster.cpp:181-184, tframe.cpp:159-160); the button deliberately does
NOT (tbutton.cpp:199-211 — the press decision uses the last move's tracked
containment, never the up position).

**The auto-repeat.** `evMouseAuto` is synthesized by
`TEventQueue::getMouseEvent` (tevent.cpp:109-204):

- on a button press: `autoTicks = downTicks = ev.what; autoDelay = repeatDelay`
  where `repeatDelay = 8` ticks (tevent.cpp:52,167-168);
- steady state with buttons down and no other event: once
  `ev.what - autoTicks > autoDelay`, fire `evMouseAuto` and set `autoDelay = 1`
  (tevent.cpp:196-201);
- tick timestamps are 55 ms BIOS ticks (`getTickCountMs() / 55`,
  hardwrvr.cpp:466-470).

Derived: **440 ms initial delay (8 × 55), then a 110 ms cadence** (the `>` on a
1-tick delay fires on the second tick boundary). A `MouseMove` does **not**
reset the cadence — the move arm updates `lastMouse` only (tevent.cpp:188-194);
only a new press re-arms. The auto event carries the current (last-known) mouse
position, with `eventFlags = 0` (tevent.cpp:124). The auto arm is the **last**
check in `getMouseEvent`, so real events always win.

**Upstream regression note.** magiblot's modern platform layer only
auto-repeats while the terminal keeps sending mouse reports
(`THardwareInfo::getMouseEvent` returns False on an empty queue,
hardware.cpp:69-78) — on a quiet terminal `evMouseAuto` starves. The widget
code (scrollbar arrows, editor drag-scroll, menus) was written against the
original Borland behavior; rstv's **timer-driven synthesizer restores it**.

## Deviation

D9 forbids nested blocking loops, so the hold loop becomes a **capture handler**
— but unlike `ButtonTrackCapture` (its row-31 predecessor, now deleted), the
capture is a **pure router, not a strategy**:

1. **`MouseAutoState` in `Program`** (src/app/program.rs) — the global
   synthesizer. The pump's event pick does the bookkeeping on every real event
   (press with a *real* button arms `now + MOUSE_AUTO_DELAY_MS`; a move updates
   position/modifiers only; up disarms) and, on an otherwise idle pick,
   synthesizes `Event::MouseAuto` at the last-known position, re-arming
   `now + MOUSE_AUTO_PERIOD_MS`. The synthesized event dispatches exactly like
   a real one. Wheel pseudo-downs (crossterm `ScrollUp/Down` → `MouseDown`
   with `wheel` set and **no buttons**) never arm. The pump's existing 20 ms
   `event_wait_timeout` (the C++ `eventTimeoutMs = 20`, tprogram.cpp:38) bounds
   auto jitter to +20 ms — the same wake cadence C++ ran its checks on.

2. **`TrackMask` + `MouseTrackCapture`** (src/capture.rs) — the seam. The
   capture holds `{ view: ViewId, origin: Point, mask: TrackMask }` and only
   ROUTES: masked `MouseMove` / `MouseAuto` / wheel pseudo-down → localize
   (subtract `origin`) and forward via `Deferred::MouseTrack`, `Consumed`;
   `MouseUp` → forward localized, `ConsumedPop` (cluster/frame read the up
   position post-loop); everything else → `Consumed` (the `mouseEvent`
   discard — the hold is modal).

3. **`Deferred::MouseTrack { view, event }`** (src/view/context.rs) — applied
   by the pump via `group.find_mut(view)` + `handle_event(&mut event, &mut ctx)`
   — the apply-time analogue of the outside-modal redirect. Direct delivery
   deliberately bypasses `Group::wants`: the C++ hold loop reads events
   straight off the queue, not through the view's `eventMask`.

4. **Widget-facing API:** `Context::start_mouse_track(view, origin, mask)`
   wraps `push_capture`; `Context::request_mouse_track` is `pub(crate)` — two
   sanctioned posters: `MouseTrackCapture` (the router for all adopters) and
   `Editor::handle_event`'s wheel-in-hold arm (the C++ `vScrollBar->handleEvent`
   / `hScrollBar->handleEvent` forwarding, teditor1.cpp:574-579).

**Why router-not-strategy (the decisive constraint).** Captures are `'static`
and hold no view borrow, so the C++ loop *body* cannot live in the capture
without duplicating widget state there (the old `ButtonTrackCapture` did
exactly that — `track_rect`, `down` — and needed two bespoke `Deferred`
variants plus a pump-side `downcast_mut::<Button>`). Worse, several B2 adopters
(`ListViewer`, `Outline`) are **trait objects** — there is no concrete type for
the pump to downcast to. Routing the localized event back into the widget's own
`handle_event` keeps the loop body *in the widget*, where the C++ put it, with
zero per-widget pump code.

**Accepted deviation: one-pump latency.** Capture pushes are deferred
(`compose_full_protocol`, capture.rs), so the capture sees the **next** event —
matching the C++ `do{}while` running the body once before the first wait. The
forwarded events themselves apply at deferred-drain time (same pump as the
capture's dispatch). This is the same latency shape the `ButtonTrackDown`
precedent already accepted.

**Origin staleness (same caveat as `DragCapture`, window.rs).** `origin` is the
absolute screen position of view-local `(0,0)`, cached from the widget's last
`draw` at push time and fixed for the hold's duration. If the tracked view were
moved/resized mid-hold the localization would go stale — acceptable: a hold is
short-lived and the modal swallow prevents anything from moving the view.

**Second-button-while-held simplification.** The C++ press arm in
`getMouseEvent` gates on `buttons == 0` (tevent.cpp:150) — a press while
another button is already held falls through to the *move* arm (position
update only), not the re-arm path. `MouseAutoState::observe` omits this gate:
a second real-button `MouseDown` while `held.is_some()` re-arms the 440 ms
delay. In practice a multi-button press during a modal hold is
user-unreachable and the only effect is resetting a timer nobody observes —
unobservable, but documented here so B2 adopters have the full contract.

## Integration (the B2 adoption recipe)

Per widget with a `do { … } while (mouseEvent(event, mask))` loop:

1. **Cache the origin in `draw`:** `self.abs_origin = ctx.origin();` (the
   `Button::abs_origin` / `ColorPicker::body_origin` pattern).
2. **`MouseDown` arm = the first loop iteration** (the C++ body runs once
   before the first wait), then enter the loop:
   `self.tracking = true; ctx.start_mouse_track(id, self.abs_origin, TrackMask { … })`
   with the mask matching the C++ call (`evMouseMove` → `mouse_move`,
   `evMouseAuto` → `mouse_auto`, `evMouse` → all three).
3. **`MouseMove` / `MouseAuto` arms = the loop body**, guarded by
   `self.tracking`. Positions arrive **view-local** (the capture localized
   them).
4. **`MouseUp` arm = the post-loop code** (+ clear the flag), also guarded.
   The localized up position is available for the callers that read it
   (cluster/frame); the button ignores it (faithful).
5. **The `tracking`-flag guard is mandatory:** `MouseUp` is not mask-gated in
   `Group::wants` (group.rs), so a stray, untracked up would otherwise hit the
   post-loop arm.

Template adopter: `Button` (src/widgets/button.rs). Constants:
`MOUSE_AUTO_DELAY_MS = 440` / `MOUSE_AUTO_PERIOD_MS = 110`
(src/app/program.rs, next to `EVENT_TIMEOUT_MS`).
