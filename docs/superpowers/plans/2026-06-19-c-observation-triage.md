# Workstream C — Secondary-Observation Triage Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Resolve the 8 secondary observations from `docs/audit/gap-report.md` §2b — two as small faithful extensions (`InputLine::set_validator`, a `Program` idle hook), seven as deliberate-absence doc notes — so workstreams B and A can document reality.

**Architecture:** Three tasks. Task 1 lands the trivial `set_validator` extension. Task 2 lands the `on_idle` hook (a `Program`-level callback fired on each event-less pump pass) after a short design spike to fix the borrow-safe call site. Task 3 writes seven deliberate-absence doc notes and records every observation's disposition in `docs/audit/gap-report.md` §2b. This plan is the **gate**: it runs before B and A.

**Tech Stack:** Rust (workspace `tvision-rs` + `tvision-rs-macros`), `insta` snapshot tests, headless backend for deterministic loop tests, mdBook/rustdoc doc gates via `cargo xtask`.

## Global Constraints

- `export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target` — artifacts land there, not `./target`.
- Workspace crate: use `--workspace` for cargo commands.
- Compile/test parallelism ≤ 4 cores: `-j 2` + `--test-threads=2`.
- English for all code/comments/identifiers.
- Commit messages end with: `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`.
- rustdoc prose is **Rust-first**; C++ lineage goes only in a `# Turbo Vision heritage` section or a `> **Turbo Vision heritage:**` blockquote.
- A new `View` trait method needs a `tvision-rs-macros/src/specs.rs` forwarder. **Neither extension here adds a `View` method** (`set_validator` is an `InputLine` method; the idle hook is a `Program` method) — so **no forwarder is needed**. Do not add one.
- Verification gates (run on the integrated tree, canonical `CARGO_TARGET_DIR`): `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`, `cargo build --examples`. For doc edits also: `cargo xtask test` (guide doctests), `cargo test --doc -p tvision-rs` (src doctests), `cargo xtask docs` (build + link-check). `cargo-insta` is not installed — generate snapshots with `INSTA_UPDATE=always`, hand-verify, commit.

---

### Task 1: `InputLine::set_validator` (extension)

Observation 5. Validator is currently constructor-only (`src/widgets/input_line.rs:137-142`); the field is `pub validator: Option<Box<dyn Validator>>` (`:107`). Add a post-construction setter so an app can swap the validator after building the field.

**Files:**
- Modify: `src/widgets/input_line.rs` (add method near the other `pub fn`s; add a unit test in the file's `#[cfg(test)]` module)

**Interfaces:**
- Consumes: existing `InputLine { validator: Option<Box<dyn Validator>>, .. }`, `Validator` trait (`src/validate.rs`).
- Produces: `pub fn InputLine::set_validator(&mut self, validator: Option<Box<dyn Validator>>)`.

- [ ] **Step 1: Write the failing test**

In the `#[cfg(test)]` module of `src/widgets/input_line.rs` (reuse the existing test imports; `FilterValidator` lives in `crate::validate`):

```rust
#[test]
fn set_validator_replaces_constructor_validator() {
    use crate::validate::FilterValidator;
    // Build with no validator, then attach one that only allows digits.
    let mut line = InputLine::new(Rect::new(0, 0, 10, 1), 9, None, LimitMode::default());
    assert!(line.validator.is_none());

    line.set_validator(Some(Box::new(FilterValidator::new("0123456789"))));
    assert!(line.validator.is_some());
    // The freshly-attached validator rejects a non-digit keystroke.
    assert!(!line.validator.as_ref().unwrap().is_valid_input("a", false));
    assert!(line.validator.as_ref().unwrap().is_valid_input("7", false));

    // Clearing it removes the constraint.
    line.set_validator(None);
    assert!(line.validator.is_none());
}
```

(If `FilterValidator::new`'s signature or `is_valid_input`'s arity differs in the current source, match the real signature in `src/validate.rs` — read it before writing the test.)

- [ ] **Step 2: Run the test to verify it fails**

Run: `cargo test -p tvision-rs --lib set_validator_replaces_constructor_validator -- --test-threads=2`
Expected: FAIL — `no method named set_validator`.

- [ ] **Step 3: Implement the setter**

Add to the `impl InputLine` block, next to `with_limit`/`select_all`:

```rust
/// Replace this field's validator after construction.
///
/// Pass `Some(validator)` to attach a [`Validator`](crate::validate::Validator)
/// that filters keystrokes and checks the field on focus-change/close, or
/// `None` to remove any constraint. The previous validator (if any) is dropped.
///
/// Most fields set their validator once via [`InputLine::new`]; use this when
/// the constraint is only known later (e.g. it depends on another control's
/// value gathered at dialog-open time).
///
/// # Turbo Vision heritage
///
/// Mirrors `TInputLine::setValidator`, which disposed the old validator and
/// assigned the new one.
pub fn set_validator(&mut self, validator: Option<Box<dyn crate::validate::Validator>>) {
    self.validator = validator;
}
```

- [ ] **Step 4: Run the test to verify it passes**

Run: `cargo test -p tvision-rs --lib set_validator_replaces_constructor_validator -- --test-threads=2`
Expected: PASS.

- [ ] **Step 5: Gate + commit**

Run: `cargo clippy --workspace --all-targets -- -D warnings -j 2 && cargo fmt --all --check`
Expected: clean.

```bash
git add src/widgets/input_line.rs
git commit -m "feat(input_line): add post-construction InputLine::set_validator

Observation 5 (docs/audit gap-report §2b): validator was constructor-only.
Faithful to TInputLine::setValidator; field was already pub.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2: `Program` idle hook (`on_idle`) (extension)

Observation 2a — the biggest concept gap. The idle arm of `pump_once` (`src/app/program.rs:1751-1806`, the `None =>` branch) runs internal idle work (command-set broadcast, timer expiry, status-line help-ctx) but exposes **no user seam**, so an app cannot run periodic work (clock, animation, heap display). The user app model is **embed-`Program`-and-call-`run_app(closure)`** (`examples/hello.rs`; `run_app` at `src/app/program.rs:765-785`) — users do NOT implement a trait. So the hook is a stored callback on `Program`, invoked from the run loop (NOT inside `pump_once`'s destructured borrow).

**Files:**
- Create: `docs/briefs/2026-06-19-on-idle-seam.md` (the design-spike output)
- Modify: `src/app/program.rs` (struct field + `set_on_idle` + run-loop call site + idle flag)
- Test: new headless unit test in the `#[cfg(test)]` module of `src/app/program.rs`

**Interfaces:**
- Consumes: `Program::run_app` / `pump_and_drive` (existing run loop), the `None =>` idle arm of `pump_once`.
- Produces:
  - `pub fn Program::set_on_idle(&mut self, f: impl FnMut(&mut Program) + 'static)`
  - field `on_idle: Option<Box<dyn FnMut(&mut Program)>>`
  - a signal that the **last pump pass was idle** (event-less), so the run loop knows when to fire the hook.

- [ ] **Step 1: Design spike — pin the borrow-safe call site (read-only, ~15 min)**

Read `src/app/program.rs` around `pump_once` (the destructured borrow of `self` fields), `pump_and_drive`, and `run_app` (`:765-785`). Answer, in `docs/briefs/2026-06-19-on-idle-seam.md` (≤1 page):
  1. Exact name/signature of the per-iteration driver (`pump_and_drive`?) and whether it already returns or can cheaply return a `bool was_idle` (i.e. the `None =>` arm ran).
  2. Where `out_events`/internal queue could make a pass "non-idle" even with no backend event (so `was_idle` means *the `None` arm actually executed*, not merely "backend returned None").
  3. The exact insertion point in `run_app`'s loop to call the hook with `&mut self` **outside** any `pump_once` borrow, using take-and-restore (`let mut h = self.on_idle.take(); if let Some(f) = h.as_mut() { f(self); } self.on_idle = h;`).
Commit the brief.

```bash
git add docs/briefs/2026-06-19-on-idle-seam.md
git commit -m "docs(brief): on_idle seam design spike (borrow-safe call site)

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

- [ ] **Step 2: Write the failing test**

In the `#[cfg(test)]` module of `src/app/program.rs`, using the headless backend pattern already used by other loop tests in that module (match the real constructor/import names you see there):

```rust
#[test]
fn on_idle_fires_each_idle_pass() {
    use std::rc::Rc;
    use std::cell::Cell;

    // Build a minimal headless Program (mirror the setup the other
    // program.rs loop tests use — same backend/clock/theme/init fns).
    let mut prog = test_program(); // <- use this module's existing helper

    let ticks = Rc::new(Cell::new(0u32));
    let ticks_in = ticks.clone();
    prog.set_on_idle(move |_p| {
        ticks_in.set(ticks_in.get() + 1);
    });

    // No events queued -> every pump pass is idle. Drive a few passes.
    for _ in 0..3 {
        prog.pump_and_drive(); // <- the real per-iteration driver name from Step 1
    }

    assert!(ticks.get() >= 3, "idle hook should fire on each event-less pass, got {}", ticks.get());
}
```

(If `program.rs` has no ready `test_program()` helper, build the `Program` inline exactly as the nearest existing loop test does — read it first; do not invent a constructor.)

- [ ] **Step 3: Run the test to verify it fails**

Run: `cargo test -p tvision-rs --lib on_idle_fires_each_idle_pass -- --test-threads=2`
Expected: FAIL — `no method named set_on_idle`.

- [ ] **Step 4: Implement the field + setter + run-loop call**

Per the Step-1 brief:
1. Add the field to the `Program` struct:

```rust
/// Optional per-idle-pass callback (see [`Program::set_on_idle`]).
on_idle: Option<Box<dyn FnMut(&mut Program)>>,
```

Initialize it to `None` everywhere `Program` is constructed (the `Program { .. }` literal(s) in `Program::new`).

2. Add the setter:

```rust
/// Register a callback run once on every **idle** pass of the event loop —
/// each iteration where no input event was waiting.
///
/// Use it for background work that should advance whenever the app is not
/// busy: a clock, an animation frame, a periodic refresh. The callback gets
/// `&mut Program`, so it can insert/close windows, post commands, or read
/// state. Keep it cheap — it runs on the loop's idle cadence (the 20 ms frame
/// tick), not a real-time scheduler. For exact timing, prefer a timer
/// ([`Event::Timer`]).
///
/// Only one idle callback is held; a second call replaces the first.
///
/// # Turbo Vision heritage
///
/// The successor to overriding `TProgram::idle`, which Turbo Vision called
/// once per event-less loop pass (the guide's clock / heap-display pattern).
pub fn set_on_idle(&mut self, f: impl FnMut(&mut Program) + 'static) {
    self.on_idle = Some(Box::new(f));
}
```

3. Make the per-iteration driver report whether the idle arm ran (e.g. `pump_and_drive(&mut self) -> bool`, returning `true` when the `None =>` arm executed). Then at the call site in `run_app`'s inner loop, after the drive:

```rust
let was_idle = self.pump_and_drive();
if was_idle {
    let mut h = self.on_idle.take();
    if let Some(f) = h.as_mut() {
        f(self);
    }
    // Restore (set_on_idle inside the callback would have replaced it).
    if self.on_idle.is_none() {
        self.on_idle = h;
    }
}
```

(Exact threading of the `bool` from `pump_once` → `pump_and_drive` is the Step-1 brief's output; follow it.)

- [ ] **Step 5: Run the test to verify it passes**

Run: `cargo test -p tvision-rs --lib on_idle_fires_each_idle_pass -- --test-threads=2`
Expected: PASS.

- [ ] **Step 6: Full gate**

Run: `cargo test --workspace -- --test-threads=2 && cargo clippy --workspace --all-targets -- -D warnings -j 2 && cargo fmt --all --check && cargo build --examples -j 2`
Expected: clean (no behavior change for apps that never call `set_on_idle`).

- [ ] **Step 7: Commit**

```bash
git add src/app/program.rs docs/briefs/2026-06-19-on-idle-seam.md
git commit -m "feat(app): add Program::set_on_idle background-work seam

Observation 2a (docs/audit gap-report §2b / concept-coverage 'biggest GAP'):
no user-facing idle hook. Adds an optional per-idle-pass callback fired from
the run loop (borrow-safe, outside pump_once). Successor to TProgram::idle.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 3: Deliberate-absence doc notes + audit disposition record (doc-only ×7)

Observations 1, 2b, 3, 4, 6, 7, 8 are deliberate divergences. Add a rustdoc note at each site (Rust-first; heritage in a `# Turbo Vision heritage` section), then record every observation's disposition in `docs/audit/gap-report.md` §2b.

**Files:**
- Modify: `src/app/program.rs` (obs 1: `desktop_insert` ~`:1238`), `src/desktop/desktop.rs` (obs 1: `insert_and_focus` ~`:195`)
- Modify: `src/backend/traits.rs` (obs 2b: `poll_event` ~`:26-77`)
- Modify: `src/widgets/static_text.rs` (obs 3: `Label` ~`:300-305`/`:405`)
- Modify: `src/widgets/editor.rs` (obs 4: `Editor` type doc; fields at `:410-415`)
- Modify: `src/menu/mod.rs` (obs 6: `command`/`command_key`/`submenu` ~`:183-229`)
- Modify: `src/validate.rs` (obs 7: `StringLookupValidator` ~`:208-228`)
- Modify: `src/widgets/mod.rs` (obs 8: module note) — or wherever the widgets module doc lives
- Modify: `docs/audit/gap-report.md` (§2b disposition record)

**Interfaces:** none (doc-only; no symbol added or changed).

- [ ] **Step 1: Obs 1 — desktop insert focus guard.** On `Program::desktop_insert` (and a one-line pointer on `Desktop::insert_and_focus`), add:

```rust
/// # Turbo Vision heritage
///
/// C++ `TGroup::insertView` disposes a window being inserted when the active
/// window cannot release focus (`validView` / `canMoveFocus`). tvision-rs does
/// **not** gate a programmatic insert: the focus-release check
/// (`valid(RELEASED_FOCUS)`) is applied where it matters interactively — Alt-N
/// window selection and modal close — not on insert. An app that inserts a
/// window expects it to appear; refusing the insert (DOS-era behavior) would
/// surprise more than it protects.
```

- [ ] **Step 2: Obs 2b — no getEvent injection seam.** On `Backend::poll_event` (or a `Program`-level note near `run_app`), add:

```rust
/// # Turbo Vision heritage
///
/// There is deliberately no app-level `getEvent` override / event-source
/// injection seam (C++ `TProgram::getEvent`). For periodic work use the timer
/// queue ([`Event::Timer`]) or [`Program::set_on_idle`]; to feed synthetic
/// input in tests, push onto the headless backend's event queue.
```

- [ ] **Step 3: Obs 3 — Label marker.** Extend the existing `Label` doc note:

```rust
/// # Turbo Vision heritage
///
/// C++ `TLabel` paints an optional monochrome focus marker in column 0 when
/// `showMarkers` (`SpecialChars`) is set. tvision-rs does not model marker
/// decoration — a label always draws the plain form and the column-0 slot is
/// filler. Focus is shown by color (the label's highlight role), which reads
/// correctly under both color and monochrome themes.
```

- [ ] **Step 4: Obs 4 — Editor per-instance search state.** On the `Editor` type doc, add:

```rust
/// Search/replace state — the find string, the replacement string, and the
/// `EF_*` option flags — is **per-editor**: each editor independently
/// remembers its own last search. (C++ Turbo Vision kept these in class-static
/// globals shared across every editor; per-instance is the intentional
/// deviation, and is usually what a user wants.)
```

- [ ] **Step 5: Obs 6 — Menu help context.** On `MenuBuilder::command` (and reference it from `command_key`/`submenu`):

```rust
/// These convenience builders use [`HelpCtx::NO_CONTEXT`]. To attach a help
/// context — or set any other non-default field — build the item explicitly
/// with [`MenuBuilder::item`] and a [`MenuItem`] literal (e.g.
/// `.item(MenuItem::Command { help_ctx: HelpCtx::custom("app.save"), .. })`).
```

- [ ] **Step 6: Obs 7 — StringLookupValidator scan.** On `StringLookupValidator`:

```rust
/// Validation is a linear scan (`O(n)`) over the list, which preserves the
/// caller's order (UI pickers may rely on it). [`new_string_list`] replaces
/// the whole list.
///
/// # Turbo Vision heritage
///
/// C++ `TStringLookupValidator` held a *sorted* collection and binary-searched
/// (`O(log n)`). For the small fixed lists these validators carry, the linear
/// scan is simpler and fast enough; order preservation is the deliberate
/// trade-off.
```

- [ ] **Step 7: Obs 8 — TMonoSelector absence.** Add to the widgets module doc (or the nearest module-level doc that lists deliberate omissions):

```rust
//! Turbo Vision's `TMonoSelector` — a cluster-based picker for monochrome
//! screen attributes, used only inside the old color dialog — is intentionally
//! not ported. The color picker was rebuilt (see [`crate::dialog::colorpick`])
//! and needs no mono-attribute selector; the general tabbed-selection idiom is
//! covered by [`TabBar`](crate::widgets::TabBar).
```

- [ ] **Step 8: Record dispositions in the audit.** In `docs/audit/gap-report.md` §2b, append the resolution to each of the 8 bullets, e.g. `— **Resolved (C):** doc-only, deliberate divergence noted on `Program::desktop_insert`.` / `— **Resolved (C):** extension — `Program::set_on_idle` landed.` Cover all 8 (obs 2 gets both the 2a-extension and 2b-doc-only notes).

- [ ] **Step 9: Doc gates**

Run: `cargo test --doc -p tvision-rs -- --test-threads=2 && cargo xtask test && cargo xtask docs`
Expected: doctests pass; guide builds; link-check clean. If a `# Turbo Vision heritage` rust block was added as a doctest, follow the hidden `# use tvision_rs as tv;` convention. (The notes above are prose, not code blocks, so no doctest is introduced — verify none of the intra-doc links you wrote are broken.)

- [ ] **Step 10: Commit**

```bash
git add src/app/program.rs src/desktop/desktop.rs src/backend/traits.rs src/widgets/static_text.rs src/widgets/editor.rs src/menu/mod.rs src/validate.rs src/widgets/mod.rs docs/audit/gap-report.md
git commit -m "docs: record deliberate-absence notes for 7 audit observations

Observations 1, 2b, 3, 4, 6, 7, 8 (docs/audit gap-report §2b): each is a
deliberate divergence from C++ Turbo Vision; document the choice + workaround
at the symbol and record the disposition in the audit.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Self-Review

- **Spec coverage:** All 8 observations covered — obs 5 (Task 1), obs 2a (Task 2), obs 1/2b/3/4/6/7/8 (Task 3). Each gets either a reviewed extension or a recorded doc disposition, satisfying the spec's "triage each, fix small ones" and the acceptance bullet "all 8 secondary observations resolved."
- **Placeholder scan:** Test bodies and doc text are concrete. The two deliberately-deferred specifics — `FilterValidator`/`test_program` exact signatures (Task 1/2) and the `was_idle` threading (Task 2) — are explicitly routed to "read the real source first" / the Step-1 design brief, which is a bounded deliverable, not a vague instruction.
- **Type consistency:** `set_validator(Option<Box<dyn Validator>>)` matches the field type at `input_line.rs:107`. `set_on_idle(impl FnMut(&mut Program) + 'static)` stored as `Option<Box<dyn FnMut(&mut Program)>>` is consistent across the field, setter, and call site. No `View`-trait method added → no `specs.rs` forwarder (stated in Global Constraints).
- **Gate ordering:** This plan is the gate; B and A start only after Task 3 commits.
