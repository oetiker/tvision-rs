# Docs User-Facing Cleanup (Phase 1) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development
> to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.
> This is a **documentation** sweep: there are no unit tests to write — each task's
> "test" is the objective gate (`cargo doc` clean + the link checker + a per-task
> invariant `grep`). No behavior changes; doc comments / `//`-comments / Markdown only.

**Goal:** Make both doc layers (rustdoc `/api/` + the mdBook guide) read for library
*users*, reflect the finished port (no "deferred"), quarantine C++ heritage, fix the
faithful↔deviations IA, promote event capture, and rename the project to `rstv`.

**Architecture:** Subagent-driven — an **editor** applies the ruleset to one file
group (grounded in that group's source so "deferred" verdicts are correct), then a
**reviewer** checks accuracy + the invariants. Rule-B class-(c) "no good reason"
findings **escalate to the orchestrator → the user**, never invented or deleted.

**Tech Stack:** Rust rustdoc (`//!`/`///`), mdBook Markdown, `cargo xtask docs`.

Spec: `docs/superpowers/specs/2026-06-12-api-docs-user-facing-cleanup-design.md`.

---

## The worker ruleset (every editing task applies this)

**Rule A — strip porting bookkeeping** from primary, API-explaining prose: row
numbers, bare `Dn` labels, `FOUNDATION`/`MECHANICAL`/`INFRA`, names of internal
porting docs (PORT-ORDER/HANDOVER/IMPLEMENTATION-LOG/PORTING-GUIDE), "breadcrumb".
Reword a *concept* plainly if it helps ("embed-and-delegate composition", not "D2").

**Rule B — "deferred" is a bug to investigate.** For each `deferred`/`not ported
yet`/`lands when…`/`TODO`, read the implementation, then:
- (a) implemented → rewrite as working;
- (b) deliberately not ported → state plainly **with the real reason** (check
  HANDOVER "latent edge notes" + surrounding code), never the word "deferred";
- (c) no good reason found → **leave a `RULEB-ESCALATE:` note in your task report**
  for the orchestrator; do not invent or delete.
- **Nuance:** the **Deferred channel / deferred effects** (the `Deferred` enum, effects
  routed to the loop owner) is a real feature — that sense STAYS. Only porting-"deferred"
  is removed.

**Rule C — heritage section** (rustdoc only; major modules + primary
`struct`/`trait`/`enum`, not every method). End the item's docs with, verbatim heading:
```rust
//! # Turbo Vision heritage
//! Ports `TXxx` (`somefile.cpp`/`header.h`). <one-line idiomatic-translation note
//! ONLY if the mapping is non-obvious — inheritance→trait, pointers→ViewId,
//! flags→bools, palette→Role; otherwise omit the note.> A linked `(deviation D8)`
//! citation is allowed HERE (heritage context), not in the API-explaining prose.
```

**Rule N — naming.** Project/product = **`rstv`** (standalone "tvision is/does…" →
"rstv"). Crate name **`tvision`** and namespace **`tv::`** unchanged. C++ origin stays
"Turbo Vision"/"magiblot/tvision". Don't blind-replace "tvision" — it is still the
crate name and the C++ project.

**Per-task gate (run before reporting done):**
```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
CARGO_BUILD_JOBS=4 cargo doc --no-deps -p tvision 2>&1 | tail -3   # must succeed
# invariant: no bookkeeping survives in THIS task's files (list them):
grep -nE '\b(row [0-9]|FOUNDATION|MECHANICAL|PORT-ORDER|HANDOVER|PORTING-GUIDE|breadcrumb)\b' <files>   # expect: nothing
grep -nE '\bdeferred\b' <files>   # expect: only Deferred-channel feature sense
```

---

## Task 1: rustdoc — `app` + `view` + `capture` (the FOUNDATION core)

**Files (modify, doc comments only):**
- `src/app/mod.rs`, `src/app/program.rs`, `src/app/application.rs`
- `src/view/view.rs`, `src/view/context.rs`, `src/view/group.rs` (+ any `view/*.rs` flagged)
- `src/capture.rs`

- [ ] **Step 1: Apply Rules A/B/C/N to each file.** Worked example — `src/app/mod.rs`:

  Before:
  ```rust
  //! Application layer — `TProgram` (row 31) and `TApplication` (row 32).
  //! [`Program`] is the application root: it owns TV's single event loop (D9),
  //! making the row-20 timer queue and the row-21 capture stack live. See
  //! [`program`] for the module docs and the deferral breadcrumbs.
  //! [`Application`] is a thin D2 embed-and-delegate wrapper over [`Program`] …
  ```
  After:
  ```rust
  //! The application layer: [`Program`], the application root that owns the
  //! single event loop, the timer queue, and the capture stack that powers
  //! modal dialogs; and [`Application`], a thin wrapper over it that adds
  //! window tiling/cascading and shell suspend.
  //!
  //! # Turbo Vision heritage
  //! Ports `TProgram` / `TApplication` (`tprogram.cpp`, `tapplica.cpp`). C++
  //! `TApplication : TProgram` inheritance becomes embed-and-delegate composition
  //! ([deviation D2](../../deviations.html#d2)) — one type holds the other and
  //! forwards to it.
  ```
  (Heritage→guide links: use the relative depth from the item's `api/` location to
  the guide's `deviations.html`; the central link check in Task 9 verifies them. If a
  correct depth is unclear, cite `(deviation D2)` without the hyperlink and add a
  `LINK-TODO:` note — Task 9 fixes.)

- [ ] **Step 2: Rule-B check.** `application.rs` claims `tile`/`cascade` are
  "deferred… lands when `Desktop::tile`/`cascade` exist" — they exist
  (`src/desktop/desktop.rs:314`/`354`) and are menu-wired. Rewrite as **implemented**.

- [ ] **Step 3: Gate** (the per-task gate above, with these files). Report any
  `RULEB-ESCALATE:`/`LINK-TODO:` notes.

- [ ] **Step 4: Commit**
  ```bash
  git add src/app src/view src/capture.rs
  git commit -m "docs(rustdoc): user-facing cleanup of app/view/capture"
  ```

## Task 2: rustdoc — `widgets` (14 files)

**Files:** every flagged file under `src/widgets/` (button, cluster, input_line,
list_viewer, outline, scrollbar, editor, file_editor, history, label, static_text,
terminal, indicator, memo, … — confirm with the grep in Step 1).

- [ ] **Step 1:** list the group's files:
  `grep -rlE '^\s*(//!|///).*(\brow [0-9]|\bD[0-9]+\b|FOUNDATION|MECHANICAL|breadcrumb|\bdeferred\b|\.cpp\b)' src/widgets`
- [ ] **Step 2:** apply Rules A/B/C/N per file. `button.rs` already has a good C++
  cross-reference + `# Model` section — keep that shape; just move the C++ bits under
  `# Turbo Vision heritage` and strip any `Dn`/row labels from the user-facing part.
- [ ] **Step 3:** Rule-B: many widgets carry `// TODO(row …)` press-and-hold notes that
  the B2 backlog already RESOLVED — verify against code; if implemented, delete the stale
  TODO; if a genuine non-port (e.g. an editor OOM path that's moot because Rust `Vec` is
  infallible), state the reason plainly.
- [ ] **Step 4:** Gate (per-task gate, widgets files).
- [ ] **Step 5:** Commit `docs(rustdoc): user-facing cleanup of widgets`.

## Task 3: rustdoc — `dialog` (7) + `menu` (5)

**Files:** flagged files under `src/dialog/` and `src/menu/`.
- [ ] Step 1: list files (grep as Task 2, those dirs).
- [ ] Step 2: Rules A/B/C/N. Step 3: Rule-B audit. Step 4: gate. Step 5: commit
  `docs(rustdoc): user-facing cleanup of dialog + menu`.

## Task 4: rustdoc — `backend` (7) + `screen` (5)

**Files:** flagged files under `src/backend/` and `src/screen/`.
- [ ] Same shape as Task 3. Commit `docs(rustdoc): user-facing cleanup of backend + screen`.

## Task 5: rustdoc — remaining modules

**Files:** flagged files in `window`, `status`, `event`, `desktop`, `validate`,
`timer`, `theme`, `text`, `help`, `frame`, `data`, `command`, `color`, `src/lib.rs`.
- [ ] Same shape. `src/lib.rs` crate-root `//!` should introduce **rstv** (Rule N) and
  the `tv::` house style for users; move any porting framing out. Commit
  `docs(rustdoc): user-facing cleanup of remaining modules`.

## Task 6: guide — strip `Dn` from prose (apps/ + internals/)

**Files:** the 23 pages flagged by `grep -rlE '\bD[0-9]+\b' docs/book/src` EXCEPT the
three handled structurally in Tasks 7–8 (`port/faithful.md`, `reference/deviations.md`,
and the new capture page). Specifically all `apps/*.md`, all `internals/*.md`, and the
`port/*.md` topic pages other than `faithful.md`.

- [ ] **Step 1:** In each page, remove bare `(D5)`/"deviation D5" labels from the
  Rust-explaining prose. Where a page has a C++-origin aside, a linked `(deviation D5)`
  citation MAY stay (Rule A exception). Apply Rule N. Keep the **Deferred channel** feature
  references (Rule B nuance) — `internals/deferred.md`, `port/deferred.md` are about the
  real feature.
- [ ] **Step 2:** Gate: `grep -rnE '\bD[0-9]+\b' <these files>` → only linked
  heritage citations remain; `cargo xtask docs` link check still clean (Task 9 is the
  full gate, but a quick `cargo run -p xtask -- docs` here catches local breakage).
- [ ] **Step 3:** Commit `docs(book): drop deviation labels from user-facing prose`.

## Task 7: guide IA — rewrite `port/faithful.md` as philosophy + gateway

**Files:** Modify `docs/book/src/port/faithful.md`.
- [ ] **Step 1:** Rewrite so it covers (1) what "faithful" means + why, and (2)
  **introduces and links** the topic chapters + the differences reference — it must NOT
  re-enumerate the deviations as its own list (that job moves to `reference/deviations.md`).
  Keep the veteran framing; drop the "D1–D13 / Baseline-Deviation-Integration / PORTING-GUIDE
  is the spec" scaffolding. End with: "For the at-a-glance mapping see
  [Differences from C++ Turbo Vision](../reference/deviations.md); for the per-topic story,
  read the chapters that follow."
- [ ] **Step 2:** Gate: `cargo run -p xtask -- docs` link check clean; `grep -nE '\bD[0-9]+\b|PORTING-GUIDE' docs/book/src/port/faithful.md` → none (or only a linked citation).
- [ ] **Step 3:** Commit `docs(book): faithful.md becomes philosophy+gateway`.

## Task 8: guide IA — `reference/deviations.md` → "Differences from C++ Turbo Vision" + capture page

**Files:** Modify `docs/book/src/reference/deviations.md`, `docs/book/src/reference/symbol-map.md`,
`docs/book/src/port/modal.md`, `docs/book/src/SUMMARY.md`; Create
`docs/book/src/port/capture.md`.

- [ ] **Step 1 (deviations.md):** Reframe as the single canonical at-a-glance list,
  titled **"Differences from C++ Turbo Vision"**. Each entry gets a **stable anchor**
  (`### Inheritance → trait + composition {#d2}` or an explicit `<a id="d2">`), a
  one-line summary, and **links** to its Part II topic page + relevant rustdoc. Replace
  "the formal spec is PORTING-GUIDE" with at most "Porting contributors: see the
  project repository." (`symbol-map.md`: same — stop treating internal docs as canonical;
  keep it a terse C++→`tv::` table.)
- [ ] **Step 2 (capture page):** Create `port/capture.md` — "Event capture — one
  mechanism for modal dialogs, window drag/resize & press-and-hold." Explain the capture
  stack as the unified mechanism (grounded in `src/capture.rs` + the ~10 widget users).
  Link to `internals/event-loop.md` + `internals/brokering.md` for mechanics.
- [ ] **Step 3 (modal.md + SUMMARY):** Narrow `modal.md` to "execView → one loop" and
  link to `capture.md`. Add the capture page to `SUMMARY.md` under Part II (after `modal.md`).
- [ ] **Step 4:** Gate: `cargo run -p xtask -- docs` link check clean (the new anchors +
  cross-links must resolve).
- [ ] **Step 5:** Commit `docs(book): differences reference + event-capture page (IA)`.

## Task 9: integrate — full gate, escalations, link fixes, invariant sweep

**Files:** whatever Tasks 1–8 flagged (`LINK-TODO:` heritage links, `RULEB-ESCALATE:` notes).

- [ ] **Step 1:** Resolve every `LINK-TODO:` — compute the correct relative depth from
  each rustdoc item to the guide `deviations.html`, or convert to the verified path.
- [ ] **Step 2:** Surface every `RULEB-ESCALATE:` to the **user** with the file, the
  claim, and what the code shows; apply their decision. (Do not guess.)
- [ ] **Step 3: Full gate.**
  ```bash
  export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
  CARGO_BUILD_JOBS=4 cargo run -p xtask --quiet -- docs        # OK + link check clean
  CARGO_BUILD_JOBS=4 cargo test --workspace -- --test-threads=4
  CARGO_BUILD_JOBS=4 cargo clippy --workspace --all-targets -- -D warnings
  cargo fmt --all --check
  ```
- [ ] **Step 4: Invariant sweep** (the success criterion):
  ```bash
  grep -rnE '\b(row [0-9]|FOUNDATION|MECHANICAL|PORT-ORDER|HANDOVER|PORTING-GUIDE|breadcrumb)\b' src docs/book/src --include=*.rs --include=*.md
  # expect: empty
  grep -rnE '\bdeferred\b' src docs/book/src --include=*.rs --include=*.md
  # expect: only the Deferred-channel feature sense
  grep -rnE '\bD[0-9]+\b' docs/book/src --include=*.md
  # expect: only linked deviation citations + the deviations.md anchors
  ```
  Every surviving hit must be intentional and explained.
- [ ] **Step 5:** Update `docs/IMPLEMENTATION-LOG.md` + `docs/HANDOVER.md` (record the
  cleanup + rename; note Phases 2–3 still pending). Commit
  `docs: integrate user-facing cleanup + rstv rename; invariants clean`.

---

## Notes for the orchestrator
- Tasks 1–5 (rustdoc) and 6 (guide prose) are **build-disjoint** across different files —
  dispatch editors concurrently, but keep `src/lib.rs` (Task 5) and `SUMMARY.md` (Task 8)
  edits to a single owner to avoid races. Two-stage review (spec/fact then quality) per task.
- Run the **full gate (Task 9)** on the integrated tree only — per-task `cargo doc` is a
  fast smoke check, not the authority (shared target dir caveat).
- `cargo-insta` snapshots are unaffected (doc-only), but Task 9 runs the suite anyway.
