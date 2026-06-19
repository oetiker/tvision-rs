# Design spike: `Program::set_on_idle` — borrow-safe call site

**Task:** add a per-idle-pass user callback (successor to `TProgram::idle`) so an
app can run background work (clock, animation, heap display) without implementing
a trait. The user app model is embed-`Program`-and-call-`run_app(closure)`, so the
hook is a stored callback on `Program`, fired from the run loop.

## Findings (read-only spike on the live source, HEAD daee212)

### 1. Per-iteration driver name/signature

The per-iteration driver is **`fn pump_and_drive(&mut self)`** (`src/app/program.rs:804`),
not a free-standing returner — it currently returns `()`. It calls `self.pump_once()`
then runs any `pending_modal` at top level. The bare iteration is
**`pub fn pump_once(&mut self)`** (`:1672`), which also returns `()` today.

`pump_once` destructures `self` into field bindings at the top (`:1673`) — an
**exhaustive** destructure (no `..`), so a callback taking `&mut Program` cannot run
inside it. The new field must be added to that destructure (bind it `on_idle: _`,
since the hook is NOT fired here).

**Plan:** thread a `bool was_idle` out:
- `pump_once(&mut self) -> bool` — returns `true` iff the `None =>` idle arm ran.
- `pump_and_drive(&mut self) -> bool` — forwards `pump_once`'s bool. (The modal
  drive after it does not change idle-ness for the hook's purpose: the hook fires
  on the pass that had no input event.)

### 2. What makes a pass "idle" (the `None` arm actually ran)

`pump_once` picks the event at `:1732`: drain the internal `out_events` queue
first, else poll the backend; then the `evMouseAuto` synthesizer (`:1743`) may
turn an empty pick into a synthesized auto. The `match ev` at `:1751` has exactly
two arms: `None =>` (the idle arm: command-set-changed broadcast, timer expiry,
status-line help-ctx update) and `Some =>` (dispatch).

So "idle" = **the final `ev` after the mouse-auto synthesizer is `None`** =
the `None =>` arm executes. A pass with a queued internal event (e.g. a re-injected
broadcast) or a synthesized mouse-auto is NOT idle. Capture `let was_idle =
ev.is_none();` immediately before the `match ev` (after the synthesizer at `:1749`)
and return it at the end of `pump_once`.

### 3. Exact insertion point in `run_app`

`run_app` (`:765`) inner loop currently:

```rust
while self.end_state.is_none() {
    self.pump_and_drive();
    let cmds: Vec<Command> = self.app_commands.drain(..).collect();
    for cmd in cmds { on_command(self, cmd); }
}
```

Change to fire the hook **outside** any `pump_once` borrow, using take-and-restore:

```rust
while self.end_state.is_none() {
    let was_idle = self.pump_and_drive();
    if was_idle {
        let mut h = self.on_idle.take();
        if let Some(f) = h.as_mut() { f(self); }
        // Restore unless the callback replaced it via set_on_idle.
        if self.on_idle.is_none() { self.on_idle = h; }
    }
    let cmds: Vec<Command> = self.app_commands.drain(..).collect();
    for cmd in cmds { on_command(self, cmd); }
}
```

`take()` moves the boxed `FnMut` out so `f(self)` holds the only `&mut self`; the
restore puts it back unless the callback itself called `set_on_idle` (which would
have set a new box — keep that, drop the old).

## Constructors / destructures touched

- Struct field `on_idle: Option<Box<dyn FnMut(&mut Program)>>` after `shell_msg_hook`
  (`:371`).
- Single real constructor `Program::new` (`:485`) `Program { .. }` literal (`:578`):
  add `on_idle: None`.
- `pump_once`'s exhaustive destructure (`:1673`): add `on_idle: _` (not fired here).
- The test-module destructures (`:6847` etc.) use `..`, so they are unaffected.

## Test (headless)

Mirror the existing `program_with_desktop(w, h)` helper (`:3518`): builds a real
headless `Program` (HeadlessBackend + ManualClock + classic_blue theme). No events
queued → every `pump_and_drive()` is idle → the hook fires each pass. New unit test
`on_idle_fires_each_idle_pass` drives 3 passes and asserts the tick count `>= 3`.

## Deviation from the brief

The brief assumed `pump_and_drive(&mut self) -> bool` and that `pump_once` might
"already return or cheaply return" the bool. Reality: both `pump_and_drive` and
`pump_once` currently return `()`; both gain a `-> bool` and the call site fires
the hook in `run_app` (not inside `pump_and_drive`). This matches the brief's
intent exactly — the bool threads `pump_once` → `pump_and_drive` → `run_app`.
