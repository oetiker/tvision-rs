# InputLine Masking + Reveal-Eye Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Give `tvision-rs` a real password field: native masking + reveal on `InputLine`, a focusable `RevealEye` toggle, and a `MaskedInput` composite that wires them together.

**Architecture:** `InputLine` gains a `mask: Option<char>` (echo char, applied only at draw) and a transient `reveal: bool`; the stored `data`/`value()` are always the real text, so caret/selection/scroll/paste stay native and there is no second copy to desync. `RevealEye` is a one-cell focusable view that drives reveal (mouse hold, or Space for a 1 s timed peek / sticky toggle). `MaskedInput` is a `Group` bundling `[InputLine, RevealEye]` with the eye in the last column, so the eye is its own Tab stop and reveal is coordinated inside one widget.

**Tech Stack:** Rust (edition 2024), tvision-rs, `insta`-free unit tests painting into a `Buffer` and asserting cells.

## Global Constraints

- Rust edition 2024; house crate alias `tv`. Build/test with **≤ 4 cores** (`cargo test -j4`).
- `InputLine` with `mask == None` must behave **byte-for-byte as today** (default off).
- Masking assumes **width-1 graphemes** (true for passwords): one echo char per `char`, and no selection-highlight repaint while masked. Document this in the doc-comments.
- `value()` / `data` always return the **real** text, never the echo.
- Glyphs: hidden `⊝` (U+229D), revealed `◉` (U+25C9). Both configurable via `RevealEyeConfig`.
- Keyboard peek default **1 s**; sticky mode is a per-field config flag (Space toggles instead of timed peek).
- New public types exported from the crate root (`src/lib.rs`); every user-visible change added under `## Unreleased` in `CHANGELOG.md`.
- Release as **0.14.0** in the final task (new feature → minor bump).
- All new files under `/home/oetiker/checkouts/rstv`.

---

## File Structure

- `src/widgets/input_line.rs` — add `mask`/`reveal` fields, setters, `masking()`/`echo_of()` helpers, draw masking, clipboard-leak guards. (modify)
- `src/command.rs` — add `Command::REVEAL_CHANGED`. (modify)
- `src/widgets/reveal_eye.rs` — new `RevealEye` view + `RevealEyeConfig`. (create)
- `src/widgets/masked_input.rs` — new `MaskedInput` composite. (create)
- `src/widgets/mod.rs` — `mod`/`pub use` the two new widgets. (modify)
- `src/lib.rs` — re-export `RevealEye`, `RevealEyeConfig`, `MaskedInput`. (modify)
- `Cargo.toml`, `CHANGELOG.md` — version bump + changelog. (modify)

---

### Task 1: `InputLine` masking state + helpers

**Files:**
- Modify: `src/widgets/input_line.rs` (struct ~84-193; both constructors ~230-289)
- Test: `src/widgets/input_line.rs` `#[cfg(test)] mod tests`

**Interfaces:**
- Produces: `InputLine.mask: Option<char>`, `InputLine.reveal: bool`, `pub fn set_mask(&mut self, mask: Option<char>)`, `pub fn set_reveal(&mut self, reveal: bool)`, `fn masking(&self) -> bool`, `fn echo_of(&self, s: &str) -> String`.

- [ ] **Step 1: Write the failing test**

Add to the tests module:

```rust
#[test]
fn mask_helpers_track_state_and_echo() {
    let mut il = InputLine::with_limit(Rect::new(0, 0, 10, 1), 64);
    assert!(!il.masking(), "unmasked by default");
    il.set_mask(Some('•'));
    assert!(il.masking(), "masking once a mask char is set");
    assert_eq!(il.echo_of("abc"), "•••", "one echo char per char");
    il.set_reveal(true);
    assert!(!il.masking(), "reveal suspends masking");
    il.set_reveal(false);
    il.set_mask(None);
    assert!(!il.masking(), "clearing the mask disables masking");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -j4 -p tvision-rs mask_helpers_track_state_and_echo`
Expected: FAIL — no method `set_mask` / `masking` / `echo_of`.

- [ ] **Step 3: Add the fields**

In `pub struct InputLine { ... }` add after `select_all_on_focus`:

```rust
    /// Echo character painted in place of the real text when set. `None` = plain
    /// field. `data`/`value()` always hold the real text; masking is draw-only.
    /// Assumes width-1 graphemes (passwords): one echo char per `char`.
    pub mask: Option<char>,
    /// Transient: when true, the real text is painted despite `mask` (reveal).
    pub reveal: bool,
```

In `pub fn new(...)` (the struct literal that returns `Self`) add `mask: None,` and `reveal: false,` alongside the other field initializers.

- [ ] **Step 4: Add the setters + helpers**

In `impl InputLine` (near `set_select_all_on_focus`):

```rust
    /// Set (or clear) the echo character. `Some(ch)` masks the display with `ch`;
    /// `None` restores the plain field. The stored value is unaffected.
    pub fn set_mask(&mut self, mask: Option<char>) {
        self.mask = mask;
    }

    /// Momentarily show the real text despite `mask` (password reveal). Ignored
    /// when the field is not masked.
    pub fn set_reveal(&mut self, reveal: bool) {
        self.reveal = reveal;
    }

    /// Whether the display is currently masked (masked and not revealed).
    fn masking(&self) -> bool {
        self.mask.is_some() && !self.reveal
    }

    /// The echo string for `s`: the mask char once per `char`. Only called while
    /// `masking()` is true, so `self.mask` is `Some`.
    fn echo_of(&self, s: &str) -> String {
        let ch = self.mask.unwrap_or('•');
        std::iter::repeat(ch).take(s.chars().count()).collect()
    }
```

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -j4 -p tvision-rs mask_helpers_track_state_and_echo`
Expected: PASS.

- [ ] **Step 6: Commit**

```bash
git add src/widgets/input_line.rs
git commit -m "feat(input): add mask/reveal state to InputLine"
```

---

### Task 2: Mask the `InputLine::draw` paint pass

**Files:**
- Modify: `src/widgets/input_line.rs` `fn draw` (lines ~816-882)
- Test: same file tests module

**Interfaces:**
- Consumes: `masking()`, `echo_of()` from Task 1.
- Produces: masked rendering — the field paints echo chars while `value()` stays real.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn masked_draw_shows_echo_not_data_and_value_is_real() {
    use crate::screen::Buffer;
    use crate::theme::Theme;
    use crate::view::DrawCtx;

    let mut il = InputLine::with_limit(Rect::new(0, 0, 8, 1), 64);
    il.set_value(FieldValue::Text("secret".into()));
    il.set_mask(Some('•'));

    let theme = Theme::classic_blue();
    let mut buf = Buffer::new(8, 1);
    {
        let mut dc = DrawCtx::new(&mut buf, &theme, Rect::new(0, 0, 8, 1), Point::new(0, 0));
        il.draw(&mut dc);
    }
    // Text is painted from column 1. Masked → bullets, never the letters.
    let row: String = (0..8).map(|x| buf.get(x, 0).symbol()).collect();
    assert!(row.contains('•'), "masked field paints bullets: {row:?}");
    assert!(!row.contains('s') && !row.contains('e'), "no cleartext: {row:?}");
    // The stored value is still the real password.
    assert_eq!(il.value(), Some(FieldValue::Text("secret".into())));

    // Revealing paints the real text.
    il.set_reveal(true);
    let mut buf2 = Buffer::new(8, 1);
    {
        let mut dc = DrawCtx::new(&mut buf2, &theme, Rect::new(0, 0, 8, 1), Point::new(0, 0));
        il.draw(&mut dc);
    }
    let row2: String = (0..8).map(|x| buf2.get(x, 0).symbol()).collect();
    assert!(row2.contains('s') && row2.contains('e'), "revealed shows real: {row2:?}");
}
```

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -j4 -p tvision-rs masked_draw_shows_echo_not_data_and_value_is_real`
Expected: FAIL — the field paints the cleartext `secret`.

- [ ] **Step 3: Modify `draw`**

Replace the base-text block (the `if size.x > 1 { ... put_str_part(0, 0, &self.data, ...) }`) so it paints the echo when masked:

```rust
    // Masked display: echo one char per character (assumes width-1 graphemes,
    // which passwords are). `self.data` / `value()` stay the real text.
    let echo = self.masking().then(|| self.echo_of(&self.data));
    let shown: &str = echo.as_deref().unwrap_or(&self.data);
    // Scrolled text from column 1, offset by first_pos.
    if size.x > 1 {
        let mut sub = ctx.sub(Rect::new(1, 0, size.x, 1));
        sub.put_str_part(0, 0, shown, self.first_pos, color);
    }
```

Then gate the selection-highlight block so it does **not** repaint while masked (a masked field shows no highlight; selection still works functionally):

```rust
    // Selection highlight (suppressed while masked so no cleartext leaks).
    if !self.masking() && self.state.state.selected && self.sel_start < self.sel_end {
        // ... existing highlight body unchanged ...
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run: `cargo test -j4 -p tvision-rs masked_draw_shows_echo_not_data_and_value_is_real`
Expected: PASS.
Then run the whole InputLine suite to prove default (unmasked) rendering is unchanged:
Run: `cargo test -j4 -p tvision-rs input_line`
Expected: PASS (existing snapshot/draw tests still green).

- [ ] **Step 5: Commit**

```bash
git add src/widgets/input_line.rs
git commit -m "feat(input): paint the echo char when masked"
```

---

### Task 3: Suppress cleartext Cut/Copy while masked

**Files:**
- Modify: `src/widgets/input_line.rs` `fn do_cut` / `fn do_copy` (lines ~622-641)
- Test: same file tests module

**Interfaces:**
- Consumes: `masking()`.
- Produces: masked Copy emits no clipboard write; masked Cut deletes without a clipboard write; Paste unaffected.

- [ ] **Step 1: Write the failing test**

```rust
#[test]
fn masked_copy_and_cut_do_not_leak_cleartext() {
    use crate::view::{Context, Deferred};
    use crate::timer::TimerQueue;
    use std::collections::VecDeque;

    fn clipboard_writes(deferred: &[Deferred]) -> usize {
        deferred.iter().filter(|d| matches!(d, Deferred::SetClipboard(_))).count()
    }

    let mut il = InputLine::with_limit(Rect::new(0, 0, 20, 1), 64);
    il.set_value(FieldValue::Text("secret".into()));
    il.set_mask(Some('•'));
    il.select_all(true, false); // select the whole value

    let mut out: VecDeque<Event> = VecDeque::new();
    let mut timers = TimerQueue::new();
    let mut deferred: Vec<Deferred> = Vec::new();
    {
        let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
        il.do_copy(&mut ctx);
    }
    assert_eq!(clipboard_writes(&deferred), 0, "masked copy must not write cleartext");

    // Cut still edits (clears the selection) but writes nothing to the clipboard.
    let mut deferred2: Vec<Deferred> = Vec::new();
    {
        let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred2);
        il.do_cut(&mut ctx);
    }
    assert_eq!(clipboard_writes(&deferred2), 0, "masked cut must not write cleartext");
    assert_eq!(il.data, "", "cut still deletes the selection");
}
```

(If `do_cut`/`do_copy` are private, this test is in the same module, so it can call them; the existing tests already call private helpers.)

- [ ] **Step 2: Run test to verify it fails**

Run: `cargo test -j4 -p tvision-rs masked_copy_and_cut_do_not_leak_cleartext`
Expected: FAIL — a `Deferred::SetClipboard` is queued.

- [ ] **Step 3: Guard the clipboard writes**

In `do_copy`, wrap the clipboard write:

```rust
    fn do_copy(&mut self, ctx: &mut Context) {
        if self.sel_start < self.sel_end && !self.masking() {
            let sel = self.data[self.sel_start as usize..self.sel_end as usize].to_string();
            ctx.set_clipboard(sel);
        }
    }
```

In `do_cut`, keep the delete but guard only the clipboard write:

```rust
    fn do_cut(&mut self, ctx: &mut Context) {
        if self.sel_start < self.sel_end {
            if !self.masking() {
                let sel = self.data[self.sel_start as usize..self.sel_end as usize].to_string();
                ctx.set_clipboard(sel);
            }
            self.save_state();
            self.delete_select();
            self.check_valid(true);
            self.sel_start = 0;
            self.sel_end = 0;
        }
    }
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -j4 -p tvision-rs masked_copy_and_cut_do_not_leak_cleartext`
Expected: PASS.

- [ ] **Step 5: Commit**

```bash
git add src/widgets/input_line.rs
git commit -m "feat(input): never copy cleartext out of a masked field"
```

---

### Task 4: `Command::REVEAL_CHANGED`

**Files:**
- Modify: `src/command.rs`

**Interfaces:**
- Produces: `Command::REVEAL_CHANGED` — a broadcast a `RevealEye` emits whenever its reveal intent changes.

- [ ] **Step 1: Add the constant**

Next to the other `pub const … : Command = Command("…");` definitions in `src/command.rs`:

```rust
    /// Broadcast by a `RevealEye` when its reveal intent changes, so the owning
    /// `MaskedInput` re-syncs its field's reveal state (source = the eye's id).
    pub const REVEAL_CHANGED: Command = Command("tv.reveal_changed");
```

- [ ] **Step 2: Verify it compiles**

Run: `cargo build -j4 -p tvision-rs`
Expected: builds clean.

- [ ] **Step 3: Commit**

```bash
git add src/command.rs
git commit -m "feat(command): add REVEAL_CHANGED broadcast"
```

---

### Task 5: `RevealEye` widget

**Files:**
- Create: `src/widgets/reveal_eye.rs`
- Modify: `src/widgets/mod.rs` (add `mod reveal_eye; pub use reveal_eye::{RevealEye, RevealEyeConfig};`)
- Modify: `src/lib.rs` (add `RevealEye, RevealEyeConfig` to the widgets re-export near line 143)
- Test: in `src/widgets/reveal_eye.rs`

**Interfaces:**
- Consumes: `Command::REVEAL_CHANGED`; `ctx.start_mouse_track`, `ctx.set_timer`, `ctx.kill_timer`, `ctx.broadcast`.
- Produces:
  - `pub struct RevealEyeConfig { pub hidden_glyph: char, pub revealed_glyph: char, pub peek: std::time::Duration, pub sticky: bool }` with `Default` = `{ '⊝', '◉', 1s, false }`.
  - `pub struct RevealEye` with `pub fn new(bounds: Rect, cfg: RevealEyeConfig) -> Self` and `pub fn is_revealing(&self) -> bool`.

- [ ] **Step 1: Write the failing tests**

Create `src/widgets/reveal_eye.rs` with only the test module first (so it fails to compile against the missing type — that is the red state):

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{Key, KeyEvent, KeyModifiers, MouseButtons, MouseEvent, MouseEventFlags, MouseWheel};
    use crate::timer::TimerQueue;
    use crate::view::{Context, Deferred, ViewId};
    use crate::{Event, Point, Rect};
    use std::collections::VecDeque;
    use std::time::Duration;

    fn eye(sticky: bool) -> RevealEye {
        let mut e = RevealEye::new(
            Rect::new(0, 0, 1, 1),
            RevealEyeConfig { sticky, ..Default::default() },
        );
        e.state_mut().id = Some(ViewId::next());
        e.state_mut().state.selected = true; // stand in for focus in a unit test
        e
    }

    fn with_ctx<R>(f: impl FnOnce(&mut Context) -> R) -> (Vec<Deferred>, R) {
        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let r = {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            f(&mut ctx)
        };
        (deferred, r)
    }

    fn mouse_down() -> Event {
        Event::MouseDown(MouseEvent {
            position: Point::new(0, 0),
            buttons: MouseButtons { left: true, ..Default::default() },
            wheel: MouseWheel::None,
            flags: MouseEventFlags::default(),
            modifiers: KeyModifiers::default(),
        })
    }
    fn mouse_up() -> Event {
        Event::MouseUp(MouseEvent {
            position: Point::new(0, 0),
            buttons: MouseButtons { left: true, ..Default::default() },
            wheel: MouseWheel::None,
            flags: MouseEventFlags::default(),
            modifiers: KeyModifiers::default(),
        })
    }

    #[test]
    fn mouse_hold_reveals_until_release() {
        let mut e = eye(false);
        with_ctx(|ctx| { let mut d = mouse_down(); e.handle_event(&mut d, ctx); });
        assert!(e.is_revealing(), "press reveals");
        with_ctx(|ctx| { let mut u = mouse_up(); e.handle_event(&mut u, ctx); });
        assert!(!e.is_revealing(), "release hides");
    }

    #[test]
    fn space_non_sticky_arms_a_timed_peek() {
        let mut e = eye(false);
        let (deferred, _) = with_ctx(|ctx| {
            let mut sp = Event::KeyDown(KeyEvent::from(Key::Char(' ')));
            e.handle_event(&mut sp, ctx);
        });
        assert!(e.is_revealing(), "Space reveals");
        assert!(
            deferred.iter().any(|d| matches!(d, Deferred::SetTimer { .. })),
            "a one-shot timer is armed"
        );
        // The matching Timer hides it again.
        let tid = e.peek_timer.expect("timer id stored");
        with_ctx(|ctx| { let mut t = Event::Timer(tid); e.handle_event(&mut t, ctx); });
        assert!(!e.is_revealing(), "timer expiry hides");
    }

    #[test]
    fn space_sticky_toggles() {
        let mut e = eye(true);
        with_ctx(|ctx| { let mut sp = Event::KeyDown(KeyEvent::from(Key::Char(' '))); e.handle_event(&mut sp, ctx); });
        assert!(e.is_revealing(), "sticky Space latches on");
        with_ctx(|ctx| { let mut sp = Event::KeyDown(KeyEvent::from(Key::Char(' '))); e.handle_event(&mut sp, ctx); });
        assert!(!e.is_revealing(), "sticky Space latches off");
    }
}
```

Note: the `Deferred::SetTimer` variant name and `MouseEvent`/`Deferred` field spellings must match the crate; if `Deferred::SetTimer` differs, adjust the matcher to the real variant (grep `enum Deferred` in `src/view/context.rs`). Confirm `ViewId::next()` exists (used in existing InputLine tests).

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -j4 -p tvision-rs reveal_eye`
Expected: FAIL to compile — `RevealEye` undefined.

- [ ] **Step 3: Implement the widget**

Prepend to `src/widgets/reveal_eye.rs` (above the tests):

```rust
//! A one-cell focusable "reveal" eye for masked [`InputLine`] fields.
//!
//! Drives a paired field's reveal: press-and-hold the mouse for a momentary
//! peek, or (while focused) press Space for a 1 s timed peek — or, when
//! configured `sticky`, toggle a latched reveal. It carries no secret text; it
//! only tracks its own intent and broadcasts [`Command::REVEAL_CHANGED`] so the
//! owning [`MaskedInput`](super::MaskedInput) can apply it.

use std::time::Duration;

use crate::command::Command;
use crate::event::{Key, TrackMask};
use crate::view::{Context, DrawCtx, Point, Rect, TimerId, View, ViewState};
use crate::{Event, Options, Role};

/// Look + behaviour of a [`RevealEye`].
#[derive(Debug, Clone, Copy)]
pub struct RevealEyeConfig {
    /// Glyph shown while the field is hidden. Default `⊝` (U+229D).
    pub hidden_glyph: char,
    /// Glyph shown while the field is revealed. Default `◉` (U+25C9).
    pub revealed_glyph: char,
    /// Timed-peek duration for a non-sticky Space press. Default 1 s.
    pub peek: Duration,
    /// When true, Space toggles a latched reveal instead of a timed peek.
    pub sticky: bool,
}

impl Default for RevealEyeConfig {
    fn default() -> Self {
        RevealEyeConfig {
            hidden_glyph: '⊝',
            revealed_glyph: '◉',
            peek: Duration::from_secs(1),
            sticky: false,
        }
    }
}

/// A focusable one-cell reveal toggle. See the module docs.
pub struct RevealEye {
    pub state: ViewState,
    cfg: RevealEyeConfig,
    /// Mouse button held on the eye (momentary peek).
    held: bool,
    /// Latched reveal (sticky mode).
    sticky_on: bool,
    /// Live one-shot peek timer (non-sticky Space), if any.
    pub(crate) peek_timer: Option<TimerId>,
    /// Absolute origin cached each `draw` for the mouse-track capture.
    abs_origin: Point,
}

impl RevealEye {
    /// Build a one-cell eye. `bounds` should be 1×1.
    pub fn new(bounds: Rect, cfg: RevealEyeConfig) -> Self {
        let mut state = ViewState::new(bounds);
        state.options = Options { selectable: true, first_click: true, ..Default::default() };
        RevealEye {
            state,
            cfg,
            held: false,
            sticky_on: false,
            peek_timer: None,
            abs_origin: Point::new(0, 0),
        }
    }

    /// Whether the paired field should currently show cleartext.
    pub fn is_revealing(&self) -> bool {
        self.held || self.sticky_on || self.peek_timer.is_some()
    }

    fn announce(&self, ctx: &mut Context) {
        ctx.broadcast(Command::REVEAL_CHANGED, self.state.id());
    }
}

impl View for RevealEye {
    fn state(&self) -> &ViewState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.state
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    fn draw(&mut self, ctx: &mut DrawCtx) {
        self.abs_origin = ctx.origin();
        let color = ctx.content_surface(
            crate::widgets::InputLine::SURFACE_ROLES,
            self.state.state.focused,
            self.state.options.selectable,
        );
        let glyph = if self.is_revealing() {
            self.cfg.revealed_glyph
        } else {
            self.cfg.hidden_glyph
        };
        ctx.fill(Rect::new(0, 0, self.state.size.x, 1), ' ', color);
        ctx.put_char(0, 0, glyph, color);
        let _ = Role::InputNormal; // keep Role import used if SURFACE_ROLES path changes
    }

    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        match ev {
            Event::MouseDown(_) => {
                if let Some(id) = self.state.id() {
                    self.held = true;
                    ctx.start_mouse_track(id, self.abs_origin, TrackMask::default());
                    self.announce(ctx);
                    ev.clear();
                }
            }
            Event::MouseUp(_) if self.held => {
                self.held = false;
                self.announce(ctx);
                ev.clear();
            }
            Event::KeyDown(k) if k.key == Key::Char(' ') => {
                if self.cfg.sticky {
                    self.sticky_on = !self.sticky_on;
                } else {
                    if let Some(t) = self.peek_timer.take() {
                        ctx.kill_timer(t);
                    }
                    self.peek_timer = Some(ctx.set_timer(self.cfg.peek, None));
                }
                self.announce(ctx);
                ev.clear();
            }
            Event::Timer(id) if Some(*id) == self.peek_timer => {
                self.peek_timer = None;
                self.announce(ctx);
                ev.clear();
            }
            // Tab, arrows, and everything else pass through for field navigation.
            _ => {}
        }
    }
}
```

Notes for the implementer:
- `InputLine::SURFACE_ROLES` is a `pub const` on `InputLine` (used in its `draw`); if it is not `pub`, define a local `SurfaceRoles { normal: Role::InputNormal, surface: Role::InputSurface, inactive: Role::InputInactive }` instead and drop the InputLine reference.
- Confirm the exact import paths (`Point`, `Rect`, `Options`, `Role`, `TrackMask`, `TimerId`) against `src/lib.rs`; adjust `use` lines to whatever the crate re-exports. `ctx.broadcast(cmd, Option<ViewId>)` matches `Dialog`'s Enter→DEFAULT broadcast.

- [ ] **Step 4: Wire the module + exports**

In `src/widgets/mod.rs` add (next to the other `mod`/`pub use`):

```rust
mod reveal_eye;
pub use reveal_eye::{RevealEye, RevealEyeConfig};
```

In `src/lib.rs`, extend the widgets re-export (near line 143) to include `RevealEye, RevealEyeConfig`.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -j4 -p tvision-rs reveal_eye`
Expected: PASS. Fix any variant/field-name mismatches surfaced by the compiler (see the notes in Steps 1 and 3).

- [ ] **Step 6: Commit**

```bash
git add src/widgets/reveal_eye.rs src/widgets/mod.rs src/lib.rs
git commit -m "feat(widget): add RevealEye reveal-toggle view"
```

---

### Task 6: `MaskedInput` composite

**Files:**
- Create: `src/widgets/masked_input.rs`
- Modify: `src/widgets/mod.rs` (`mod masked_input; pub use masked_input::MaskedInput;`)
- Modify: `src/lib.rs` (re-export `MaskedInput`)
- Test: in `src/widgets/masked_input.rs`

**Interfaces:**
- Consumes: `InputLine` (Task 1-3), `RevealEye`/`RevealEyeConfig` (Task 5), `Group`, `#[delegate(to = group)]`.
- Produces:
  - `pub fn new(bounds: Rect, limit: i32, mask: char, cfg: RevealEyeConfig) -> Self`
  - `pub fn value(&mut self) -> Option<FieldValue>` (the real text)
  - internal `input_id`, `eye_id`; the eye occupies the field's last column; the eye is a Tab stop; reveal is applied to the input after every event.

- [ ] **Step 1: Write the failing tests**

Create `src/widgets/masked_input.rs` with the test module:

```rust
#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::{MouseButtons, MouseEvent, MouseEventFlags, MouseWheel, KeyModifiers};
    use crate::timer::TimerQueue;
    use crate::view::{Context, Deferred, ViewId};
    use crate::{Event, FieldValue, Point, Rect};
    use std::collections::VecDeque;

    fn with_ctx<R>(mi: &mut MaskedInput, f: impl FnOnce(&mut MaskedInput, &mut Context) -> R) -> R {
        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
        f(mi, &mut ctx)
    }

    fn built() -> MaskedInput {
        let mut mi = MaskedInput::new(Rect::new(0, 0, 12, 1), 64, '•', RevealEyeConfig::default());
        // Give real ids as Group::insert would in a live tree, then seed a value.
        mi.set_value(FieldValue::Text("pw".into()));
        mi
    }

    #[test]
    fn value_returns_the_real_text() {
        let mut mi = built();
        assert_eq!(mi.value(), Some(FieldValue::Text("pw".into())));
    }

    #[test]
    fn eye_press_reveals_the_paired_input() {
        let mut mi = built();
        // Press the eye (its own cell = last column). Route through the group.
        let down = Event::MouseDown(MouseEvent {
            position: Point::new(11, 0),
            buttons: MouseButtons { left: true, ..Default::default() },
            wheel: MouseWheel::None,
            flags: MouseEventFlags::default(),
            modifiers: KeyModifiers::default(),
        });
        with_ctx(&mut mi, |mi, ctx| { let mut d = down; mi.handle_event(&mut d, ctx); });
        assert!(mi.input_ref().reveal, "holding the eye reveals the input");

        let up = Event::MouseUp(MouseEvent {
            position: Point::new(11, 0),
            buttons: MouseButtons { left: true, ..Default::default() },
            wheel: MouseWheel::None,
            flags: MouseEventFlags::default(),
            modifiers: KeyModifiers::default(),
        });
        with_ctx(&mut mi, |mi, ctx| { let mut u = up; mi.handle_event(&mut u, ctx); });
        assert!(!mi.input_ref().reveal, "releasing hides the input again");
    }

    #[test]
    fn input_occupies_all_but_the_last_column() {
        let mi = built();
        let ib = mi.input_bounds();
        assert_eq!(ib, Rect::new(0, 0, 11, 1), "input is width-1, eye holds the last column");
    }
}
```

Add `input_ref(&mut self) -> &InputLine`, `input_bounds(&mut self) -> Rect` as `#[cfg(test)]` helpers if you prefer; or make them plain private helpers used by both code and tests.

- [ ] **Step 2: Run to verify it fails**

Run: `cargo test -j4 -p tvision-rs masked_input`
Expected: FAIL to compile — `MaskedInput` undefined.

- [ ] **Step 3: Implement the composite**

Prepend to `src/widgets/masked_input.rs`:

```rust
//! A masked text field with a reveal eye: an [`InputLine`] (masked) plus a
//! [`RevealEye`](super::RevealEye) in its last column, bundled as one group so
//! the eye is its own Tab stop and reveal is wired internally.

use crate::view::{Context, DrawCtx, FieldValue, Group, Rect, View, ViewId, ViewState};
use crate::widgets::{InputLine, RevealEye, RevealEyeConfig};
use crate::Event;

/// See the module docs.
pub struct MaskedInput {
    group: Group,
    input_id: ViewId,
    eye_id: ViewId,
}

impl MaskedInput {
    /// Build a masked field of `bounds` with byte cap `limit`, echo char `mask`,
    /// and reveal-eye config `cfg`. The input takes all but the last column; the
    /// eye takes the last column.
    pub fn new(bounds: Rect, limit: i32, mask: char, cfg: RevealEyeConfig) -> Self {
        let w = bounds.b.x - bounds.a.x;
        let h = bounds.b.y - bounds.a.y;
        let mut group = Group::new(bounds);

        let mut input = InputLine::with_limit(Rect::new(0, 0, (w - 1).max(1), h), limit);
        input.set_mask(Some(mask));
        let input_id = group.insert(Box::new(input));

        let eye = RevealEye::new(Rect::new((w - 1).max(1), 0, w, h), cfg);
        let eye_id = group.insert(Box::new(eye));

        MaskedInput { group, input_id, eye_id }
    }

    fn input_mut(&mut self) -> Option<&mut InputLine> {
        self.group.child_mut(self.input_id)?.as_any_mut()?.downcast_mut::<InputLine>()
    }
    fn eye_mut(&mut self) -> Option<&mut RevealEye> {
        self.group.child_mut(self.eye_id)?.as_any_mut()?.downcast_mut::<RevealEye>()
    }

    /// The real (unmasked) field value.
    pub fn value(&mut self) -> Option<FieldValue> {
        self.input_mut().and_then(|i| i.value())
    }
    /// Set the field value.
    pub fn set_value(&mut self, v: FieldValue) {
        if let Some(i) = self.input_mut() {
            i.set_value(v);
        }
    }

    #[cfg(test)]
    fn input_ref(&mut self) -> &InputLine {
        self.input_mut().expect("input child")
    }
    #[cfg(test)]
    fn input_bounds(&mut self) -> Rect {
        self.input_mut().expect("input child").state().get_bounds()
    }
}

#[crate::delegate(to = group)]
impl View for MaskedInput {
    fn state(&self) -> &ViewState {
        self.group.state()
    }
    fn state_mut(&mut self) -> &mut ViewState {
        self.group.state_mut()
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    fn draw(&mut self, ctx: &mut DrawCtx) {
        // Active-line only: the eye shows while this field has focus.
        let focused = self.group.state().state.focused;
        if let Some(eye) = self.eye_mut() {
            eye.state_mut().state.visible = focused;
        }
        self.group.draw(ctx);
    }

    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        // Group routes the event to the focused child (input caret or eye),
        // handles Tab between them, and fans Timer/Broadcast to both.
        self.group.handle_event(ev, ctx);
        // Re-sync the input's reveal from the eye after every event. The eye
        // broadcasts REVEAL_CHANGED on any intent change, so this pass runs
        // promptly even for a capture-delivered MouseUp.
        let revealing = self.eye_mut().map(|e| e.is_revealing()).unwrap_or(false);
        if let Some(input) = self.input_mut() {
            input.set_reveal(revealing);
        }
    }
}
```

Implementer notes:
- Confirm `Group::insert` returns `ViewId` (research §6). Confirm `state().get_bounds()` exists on `ViewState` (used widely). Adjust `Rect` field access (`bounds.b.x - bounds.a.x`) to the crate's `Rect` API if it exposes `width()`/`height()` helpers.
- The `draw` eye-visibility toggle is the "active line only" behaviour; if a consumer wants the eye always visible, that becomes a future config flag (out of scope here).

- [ ] **Step 4: Wire the module + exports**

`src/widgets/mod.rs`:

```rust
mod masked_input;
pub use masked_input::MaskedInput;
```

`src/lib.rs`: add `MaskedInput` to the widgets re-export.

- [ ] **Step 5: Run the tests to verify they pass**

Run: `cargo test -j4 -p tvision-rs masked_input`
Expected: PASS. Resolve any API-name mismatches the compiler reports.

- [ ] **Step 6: Commit**

```bash
git add src/widgets/masked_input.rs src/widgets/mod.rs src/lib.rs
git commit -m "feat(widget): add MaskedInput (masked field + reveal eye)"
```

---

### Task 7: Full-suite check, changelog, version bump to 0.14.0

**Files:**
- Modify: `CHANGELOG.md`, `Cargo.toml`

- [ ] **Step 1: Run the whole suite + clippy**

Run: `cargo test -j4 -p tvision-rs`
Expected: PASS.
Run: `cargo clippy -j4 --all-targets -- -D warnings`
Expected: no warnings.

- [ ] **Step 2: Changelog**

Under `## Unreleased` in `CHANGELOG.md`, add to `### New`:

```markdown
- `InputLine` gains password masking: `set_mask(Some(ch))` paints an echo
  character while `value()` keeps the real text, `set_reveal(bool)` momentarily
  shows it, and Cut/Copy never place cleartext on the clipboard while masked.
- New `RevealEye` toggle and `MaskedInput` composite: a masked field with a
  reveal eye in its last column (mouse press-and-hold to peek; Space for a timed
  or, when `sticky`, latched reveal). Glyphs and peek duration are configurable
  via `RevealEyeConfig`.
```

- [ ] **Step 3: Version bump**

In `Cargo.toml`, set:

```toml
version = "0.14.0"
```

Move the `## Unreleased` block to `## 0.14.0 - 2026-07-24` in `CHANGELOG.md` (leave a fresh empty `## Unreleased` with `### New`/`### Changed`/`### Fixed` above it), matching the existing style.

- [ ] **Step 4: Verify build**

Run: `cargo build -j4 -p tvision-rs`
Expected: builds clean at 0.14.0.

- [ ] **Step 5: Commit + tag**

```bash
git add CHANGELOG.md Cargo.toml
git commit -m "release: v0.14.0 — InputLine masking + reveal eye"
git tag v0.14.0
```

(Publishing to crates.io is a separate manual step the maintainer runs; the edaptor plan begins against a `path = "../rstv"` dependency until then.)

---

## Notes for the edaptor adoption plan (written after this lands)

- edaptor's `MaskedInputLine` (mirror) is deleted; New/Confirm become `MaskedInput::new(rect, 8192, '•', RevealEyeConfig::default())` — non-sticky (Space = 1 s peek).
- `PasswordDialog` reads cleartext via `MaskedInput::value()`; the New==Confirm `valid()` gate stays.
- Point `tvision-rs` at `path = "../rstv"` while iterating; pin `0.14.0` once published.
