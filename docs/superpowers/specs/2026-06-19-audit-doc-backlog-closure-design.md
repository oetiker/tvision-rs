# Audit documentation-backlog closure (design)

**Date:** 2026-06-19
**Status:** design approved, awaiting spec review → writing-plans
**Predecessor:** `docs/superpowers/specs/2026-06-18-tv2-guide-coverage-audit-design.md`
(the AUDIT round — read-only; produced the discovery artifacts under `docs/audit/`).
**This round:** the **fix** round the audit deferred to "a separately-planned
follow-up driven by the artifacts."

## Goal

Drive the audit's documentation backlog to closure. Concretely:

1. Every below-bar public symbol in `docs/audit/rustdoc-scorecard.md` reaches the
   **score-3 bar** — "what it does **+ how/when to use it**" (with a
   `# Turbo Vision heritage` section where the C++ lineage aids understanding).
2. Every Part-2 concept GAP / partial-GAP / `→ concept` route in
   `docs/audit/concept-coverage.md` gets the missing mdBook narrative.
3. The 8 secondary observations in `docs/audit/gap-report.md` §2b are **triaged**
   and resolved — each as *doc-only* (deliberate-absence + workaround) or a
   *small faithful extension*.

## What the audit already settled (do not re-litigate)

- **Code coverage is complete.** `gap-report.md`: **0 missing · 0 suspect**. The
  461 `NOT-PORTED` entries are a permanent do-not-re-flag register. This round
  adds **no** re-ports of NOT-PORTED items.
- The doc debt is real and quantified: **~543–644 public symbols below the
  "what + how" bar** (1 undocumented · 36 signature-only · ~607 what-but-not-how),
  routed to rustdoc; plus the Part-2 concept gaps routed to the mdBook.
- The two-axis routing is deliberate: **per-symbol reference → rustdoc; concepts
  → mdBook** (mirrors the guide's Part 3 vs Part 2 split).

## The three workstreams

Executed in the order **C → B → A**. C's outcomes (does an `on_idle` seam exist?
does `set_validator` exist?) determine what B and A must describe, so it gates
them. B (the narrative) precedes A so that rustdoc `→ concept` notes can link
into freshly-written chapters.

### Workstream C — secondary-observation triage (the gate)

Source: `docs/audit/gap-report.md` §2b. **One plan, 8 rows.** For each: decide
*doc-only* vs *small faithful extension*, then land it (extension = full subagent
loop + gates + snapshot if it draws; doc-only = a deliberate-absence note with the
workaround). The 8:

| # | Observation | Likely disposition |
|---|---|---|
| 1 | `TProgram` window-insert `CanMoveFocus`/`ValidView` guard not applied at `desktop_insert` (C++ disposes a window whose active sibling can't release focus) | triage — small extension or documented divergence |
| 2 | No user-facing **idle/`on_idle`** seam **and** no `getEvent`/event-source **injection** seam (the biggest concept GAP; app can't run a clock/animation/background pass) | **primary extension candidate** |
| 3 | `TLabel` focus-marker glyph never rendered (ShowMarkers/SpecialChars monochrome column-0 marker) | triage — small extension or documented divergence |
| 4 | Editor find/replace flags per-instance vs C++ class-static (shared across editors) | likely doc-only (deliberate, document it) |
| 5 | `TInputLine` no post-construction `set_validator` (validator is ctor-only) | likely doc-only (ownership choice) — or add setter |
| 6 | `TMenu`/MenuBuilder `submenu()`/`command()` hardcode `HelpCtx::NO_CONTEXT` (no escape hatch) | triage — small builder param vs doc-only |
| 7 | `TStringLookupValidator` linear scan vs C++ binary search; `new_string_list(nil)` free-vs-replace semantics | likely doc-only (note O(n); behavior is correct) |
| 8 | `TMonoSelector` no user-facing picker for mono attributes | likely doc-only (only mattered inside the superseded color dialog) |

The triage **decision + rationale** for each is recorded (so a future audit does
not re-flag a deliberate doc-only call). Any new `View` trait method added by an
extension needs its `tvision-rs-macros/src/specs.rs` forwarder; a new `Deferred`
variant needs none.

### Workstream B — mdBook concept chapters

Source: `docs/audit/concept-coverage.md`. **One plan**, one row per gap → target
`.md` file + content to add. Items:

**Hard GAPs (capability present in code, no chapter explains the mechanism):**

- Local ↔ global coordinate translation for a custom-view author (no named
  `make_local`/`make_global`; the router subtracts child origin inline) → likely
  `internals/custom-view.md` / `internals/view-tree.md`.
- Block vs underline cursor shape (`sfCursorIns` → `ViewState::cursor_ins`) →
  `internals/event-loop.md` or `apps/text-editing.md`.
- Abandoned-event / `eventError` path (unhandled events fall through the pump) →
  `port/events.md` / `internals/event-loop.md`.
- Override `getEvent` / inject an extra event source / transform the stream
  app-wide → `internals/event-loop.md` (content depends on C#2's outcome).
- Idle-time / background processing — how an app runs work each idle pass
  (clock/animation/heap display) → new narrative in `internals/event-loop.md`
  (content depends on C#2's outcome).

**Partial-GAPs (covered thinly; draw out the mechanism):** setState-override
reaction · drag-limit-bit semantics · grow-mode anchor-edges model · general
Z-reorder primitive · clip-rect-driven partial draw · why a view reads `phase()`
and reacts (Alt-letter pre vs plain post) · event-mask opt-in of expensive
classes (narrative) · `clearEvent` who-handled recording · inter-view messaging /
broadcast-as-message probe idiom · hint-by-context override · change-directory
dialog coverage · tab-order = transfer-order tie · outline in an apps chapter ·
terminal in an apps chapter · history persistence (store/load) idiom ·
`ofValidate` focus-hold behavior · validate-on-demand `valid(cmClose)` ·
custom-view colors via `Role` recipe · editor command self-gating ·
WordStar/Ctrl-K binding enumeration · editor find/replace flow · Memo-as-control
usage.

**`→ concept` routes (6) from the per-symbol reference:** `TGroup` Phase ·
`TGroup` EndModal · `TGroup` ExecView · `TGroup` Execute ·
`TStringLookupValidator` Error · `TView` DrawView. Each routes to the relevant
existing chapter (`port/events.md`, `port/modal.md`, `internals/event-loop.md`,
`internals/deferred.md`, `port/draw.md`).

Authoring follows the Docs Phase-1/3 conventions: **Rust-first prose**, C++
demoted to a `> **Turbo Vision heritage:**` blockquote; any new `rust` block uses
the hidden `# use tvision_rs as tv;` (+ `# fn _demo(recv){…}`) doctest convention
and must pass `cargo xtask test`.

### Workstream A — rustdoc score-3 sweep (a playbook, not per-section plans)

The work is near-identical per section, and the audit's `reference/<Section>.md`
files already enumerate, per symbol, what is missing. So A is **one general
playbook** applied uniformly; the audit files **are** the per-section task lists.
Batches (the audit groupings 1–11) are only parallel-dispatch units and merge
checkpoints — **not** separate planning documents.

**The playbook (written once, in A's plan):**

1. Read the section's `docs/audit/reference/<Section>.md`; collect every row with
   doc score < 3.
2. Read the cited `tv::` symbol(s) in source; read the magiblot C++ for original
   intent where the heritage note needs it.
3. For each below-bar symbol, write rustdoc to **score 3**: a Rust-first sentence
   on *what*, then *how/when* to use it; add a `# Turbo Vision heritage` section
   only where the C++ lineage clarifies. `pub(crate)`/private symbols on the
   scorecard get an internal doc comment but are not held to the public bar.
4. If a row carries `→ concept`, add a short rustdoc note **linking** to the B
   chapter rather than inlining the concept.
5. Verify: `cargo test --doc -p tvision-rs`, `cargo xtask docs` (build +
   link-check), `cargo fmt --check`. Re-score the section's rows ≥3 in the audit
   file.

**Order within A:** the **priority queue first** (the 37 score-0/1 symbols in the
scorecard), then score-2 sections **by descending count** (Globals-363-378 38,
Globals-331-346 33, TView 32, Globals-347-362 31, TOutlineViewer 27, …).

## Methodology & process

- **Subagent-driven** (project standard): fresh implementer → spec-compliance
  review → quality review → integrate on the canonical tree → commit. **Model by
  workstream:** Sonnet for the mechanical rustdoc batches (A); Opus / main-thread
  for C triage decisions and B narrative authoring.
- **Parallelism:** A's batches are file-disjoint by section and may dispatch
  concurrently via `isolation: "worktree"`; the orchestrator owns shared-file
  edits (`lib.rs`, `mod.rs`, `SUMMARY.md`) and all merges. Re-verify on the
  integrated tree (worktree "clean" claims are unreliable on a shared target dir).
- **Incremental merge:** each section/chapter/observation merges as it passes,
  branch-first, fast-forward; commit at batch boundaries before dispatching
  worktree agents that build on the prior batch.
- **Burn-down in place:** as items close, update the score/coverage cells in the
  `docs/audit/` files so the audit doubles as the live tracker; add an
  `IMPLEMENTATION-LOG.md` section per landed batch.

## Verification gates

- **Every round:** `cargo xtask test` (guide doctests — real rustc errors),
  `cargo test --doc -p tvision-rs` (src doctests), `cargo build --examples`,
  `cargo xtask docs` (build + owned book↔api link check; grep the built HTML for
  leftover `rustdoc_include` directives after any include edit).
- **Any C extension also:** `cargo test --workspace`, `cargo clippy --workspace
  --all-targets -- -D warnings`, `cargo fmt --all --check`, and a `insta`
  snapshot test if it draws (generate with `INSTA_UPDATE=always`, hand-verify,
  commit — `cargo-insta` is not installed).
- Compile/test parallelism stays ≤ 4 cores (`-j 2` + `--test-threads=2` per
  agent; at most two building agents at once), each parallel agent with its own
  `CARGO_TARGET_DIR`.

## Decomposition into plans

This spec is the umbrella. It yields **three plans**:

1. **C — observation triage** (8 rows; the gate).
2. **B — concept chapters** (one row per GAP/partial-GAP/route).
3. **A — rustdoc sweep** (the playbook + the batch/priority schedule).

Each plan is executed via `superpowers:executing-plans` /
`superpowers:subagent-driven-development`. A is written once and applied per
section; B and C are per-item because their items genuinely differ.

## Explicitly out of scope

- Any re-port of a `NOT-PORTED` item, or any new feature beyond the small
  faithful extensions that the C triage explicitly approves.
- Re-running or re-deriving the audit classifications (trusted as the baseline;
  the audit's own self-verification + anti-hallucination spot check already ran).
  A section's auditor call may be corrected in passing if a symbol moved, but
  wholesale re-audit is not in scope.
- Scoring/grading existing mdBook chapters (B *fills named gaps*; it does not
  re-grade chapters).
- The 107 pre-existing broken intra-doc rustdoc links (a separate, already-noted
  cleanup) — touch only if a section under edit owns them.

## Acceptance (this round is done when)

- All 8 secondary observations are resolved — each either documented as a
  deliberate absence (with workaround + recorded rationale) or landed as a
  reviewed faithful extension.
- Every hard GAP, partial-GAP, and `→ concept` route in `concept-coverage.md` has
  narrative in the named mdBook chapter; rustdoc `→ concept` rows link to it.
- Every public symbol on `rustdoc-scorecard.md` re-scores ≥ 3; the audit files
  are updated to reflect the new scores (no stale "< 3" cells for closed items).
- All verification gates are green on the integrated tree.
- `IMPLEMENTATION-LOG.md` records each landed batch; `HANDOVER.md` "Next" is
  updated to reflect remaining (if any) and the closed state.
