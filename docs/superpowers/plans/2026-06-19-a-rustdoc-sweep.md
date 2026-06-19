# Workstream A — rustdoc Score-3 Sweep Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan. Unlike a normal plan, **A is one playbook applied per section** — the audit's `reference/<Section>.md` files are the per-section task lists. The "tasks" below are the playbook (run once per section) plus the batch/priority schedule.

**Goal:** Raise every below-bar public symbol in `docs/audit/rustdoc-scorecard.md` (~543–644 across ~80 sections) to score-3 — "what it does **+ how/when to use it**" — and re-score the audit files to reflect closure.

**Architecture:** A single repeatable **playbook** (Task P) is applied to each reference section. Sections are file-disjoint and dispatched concurrently in **batches** (the audit groupings) via `isolation: "worktree"`; the orchestrator integrates on the canonical tree. Order: the score-0/1 **priority queue** first, then score-2 sections by descending count. Six `→ concept` rows defer to their workstream-B anchor.

**Tech Stack:** rustdoc, `cargo test --doc`, `cargo xtask docs` (build + book↔api link-check).

## Global Constraints

- `export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target`. **Each parallel worktree agent gets its own** `CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target-<tag>` — a shared dir makes "clean" claims unreliable; the orchestrator re-verifies on the canonical tree.
- Worktrees live under `/scratch/oetiker/claude-worktrees/rstv-<tag>`. Commit completed batches before dispatching worktree agents that build on them. Run `git merge` only in `/home/oetiker/checkouts/rstv`, never inside a worktree.
- Compile/doc parallelism ≤ 4 cores: at most two building agents at once, `-j 2` each.
- rustdoc prose is **Rust-first**; C++ lineage only in a `# Turbo Vision heritage` section. Strip porting bookkeeping (row numbers, D-labels, FOUNDATION/MECHANICAL) from any rustdoc touched.
- `pub(crate)`/private symbols on the scorecard get a useful internal comment but are **not** held to the public score-3 bar (note visibility; do not invent public API).
- Any new ` ```rust ` doctest in rustdoc must compile under `cargo test --doc -p tvision-rs`.
- Commit messages end with: `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`.

### Cross-workstream dependency edges

- **A-after-C (file conflicts):** sections whose Rust module is among C's edited files must merge **after** workstream C lands. C's files: `src/app/program.rs`, `src/desktop/desktop.rs`, `src/backend/traits.rs`, `src/widgets/static_text.rs`, `src/widgets/editor.rs`, `src/menu/mod.rs`, `src/validate.rs`, `src/widgets/input_line.rs`, `src/widgets/mod.rs`. Affected batches: **App/desktop, Controls-I, Menus, Editor, Validators.** All other batches are C-independent and may start immediately.
- **A-after-B (concept links):** the six `→ concept` rows — `TGroup` Phase/EndModal/ExecView/Execute, `TStringLookupValidator` Error, `TView` DrawView — add a short rustdoc note **linking** to the B anchor (`#the-phase-field`, `#ending-a-modal-execview`, `#the-modal-loop-execute`, `#endmodal`, `#validator-error-dialogs`, `#draw-on-demand-vs-whole-tree`). Do these rows **after** the matching B chapter is on the tree; the rest of those sections need not wait.

---

## Task P: the per-section playbook (run once per section)

For one reference section `<Section>` (e.g. `TButton`):

- [ ] **P1 — collect the work.** Open `docs/audit/reference/<Section>.md`; list every row with **doc score < 3** and its cited `tv::` symbol. (For the priority-queue pass, also pull that section's entries from `rustdoc-scorecard.md`'s score-0/1 table.)
- [ ] **P2 — read the code.** Read each cited symbol in source; read the magiblot C++ only where a heritage note needs the original intent.
- [ ] **P3 — write to score-3.** For each below-bar **public** symbol, rewrite its rustdoc to: one Rust-first sentence on **what**, then **how/when** to use it (the missing half); add a `# Turbo Vision heritage` section only where the C++ lineage clarifies. For `→ concept` rows, add the one-line note linking the B anchor instead of inlining the concept. For `pub(crate)`/private rows, add a useful internal comment (not held to the public bar).
- [ ] **P4 — verify.** `cargo test --doc -p tvision-rs -- --test-threads=2` (new doctests compile/pass); `cargo fmt --all --check`; for the integrated re-check, `cargo xtask docs` (build + link-check — a `→ concept` link to a missing B anchor fails here, which is the intended guard).
- [ ] **P5 — re-score the audit.** In `docs/audit/reference/<Section>.md`, update each addressed row's doc cell to its new score (≥3, or `N/A` for private); update the section's `doc<3` rollup. The audit thus tracks the burn-down.
- [ ] **P6 — commit** (one commit per section, or one per batch at the orchestrator's discretion):

```bash
git add src/<module(s)> docs/audit/reference/<Section>.md
git commit -m "docs(rustdoc): <Section> — raise below-bar symbols to score-3

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

**Per-section acceptance:** every public row in `<Section>.md` re-scores ≥3; no `→ concept` row left without a working B link; gates green.

---

## Task S: the schedule (which sections, in which order)

Apply Task P to sections in this order. Within a batch, sections are file-disjoint → dispatch concurrently in worktrees (≤2 building at once).

- [ ] **Batch 0 — priority queue (score 0/1, 37 symbols).** Clear every row in `rustdoc-scorecard.md`'s "score 0 & 1" table first (highest impact per edit). These span Globals-317-330/331-346/347-362/363-378/582-586, TApplication, TCluster, TColorDialog/Display/ItemList, TCommandSet, TFileEditor, TInputLine, TMenu, TOutlineViewer, TStatusLine. **Honor the A-after-C edge:** the TInputLine / TFileEditor / TMenu / Globals rows that live in C's files merge after C. Do the C-independent score-0/1 rows immediately.
- [ ] **Batch 1 — Base/primitives (C-independent, start now):** TView (32), TRect (11), TPoint (7), TGroup (9, minus the Phase/EndModal/ExecView/Execute `→ concept` rows → after B Task 1/5), TFrame (3), TDrawBuffer (2), TPalette (1), PrimitiveTypes (6), TObject. *Highest single-section payload (TView 32) lives here.*
- [ ] **Batch 2 — Lists & scroll (C-independent):** TOutlineViewer (27, minus done score-1 rows from Batch 0), TListViewer (12), TScrollBar (13), TScroller (11), TListBox (5), TSortedListBox (5), THistory (7)/THistoryViewer/THistoryWindow, TIndicator (7).
- [ ] **Batch 3 — Globals & events (C-independent except backend/traits.rs):** Globals-363-378 (38), Globals-331-346 (33), Globals-347-362 (31), Globals-317-330 (18), Globals-582-586 (6), TEvent (20). *Largest score-2 counts — heaviest batch; split across multiple agents.*
- [ ] **Batch 4 — Outline & misc (C-independent):** TOutline (11), TNode (6), TMultiCheckBoxes (10), TButton (8), TCheckBoxes (5), TRadioButtons (6), TStaticText (2, shares static_text.rs → after C), TParamText (3, → after C).
- [ ] **Batch 5 — App/desktop (A-after-C):** TProgram (6), TApplication (9), TDesktop (3), TBackground (2), TWindow (15), TDialog (6).
- [ ] **Batch 6 — Controls-I (A-after-C):** TInputLine (14), TCluster (20), TLabel (4), TFileInputLine (4).
- [ ] **Batch 7 — Menus (A-after-C):** TMenuItem (12), TMenuView (8), TMenuBar (2), TMenuBox (3), TStatusLine (8), TStatusDef (4), TStatusItem (3).
- [ ] **Batch 8 — Editor & text (A-after-C):** TEditor (10), TFileEditor (11), TMemo (4), TEditWindow (6), TTerminal (7), TTextDevice (2), TMemoData.
- [ ] **Batch 9 — Dialogs & files (mixed; non-C files start now):** TFileDialog (21), TChDirDialog (6), TFileList (6), TFileInfoPane (3), TDirListBox (5), TDirEntry (5), TDirCollection (1), TFileCollection (4), TSearchRec (3), TColorDialog/Display/ItemList remainder.
- [ ] **Batch 10 — Validators (A-after-C):** TValidator (3), TFilterValidator (4), TRangeValidator (3), TLookupValidator (1), TStringLookupValidator (3, Error row → after B Task 8), TPXPictureValidator (2).
- [ ] **Batch 11 — Remaining low-count sections (C-independent):** TScrollChars, TStrListMaker, TStringList, TVTransfer, TFrame remainder, and any section with `doc<3` ≥1 not yet covered. Reconcile against `coverage-matrix.md`'s `doc<3` column so none is skipped.

- [ ] **Final reconciliation.** After all batches: confirm `rustdoc-scorecard.md` "Total below bar" is reduced to only intentionally-private rows; update its headline counts and the `coverage-matrix.md` `doc<3` column; `cargo xtask docs` clean with zero broken `→ concept` links; add an `IMPLEMENTATION-LOG.md` section and update `HANDOVER.md` "Next".

---

## Self-Review

- **Spec coverage:** The playbook + schedule address every section with a `doc<3` count in `coverage-matrix.md`; Batch 0 front-loads the 37 score-0/1 rows; the final reconciliation forces no-silent-skip against the matrix. Matches the spec's "every below-bar symbol re-scores ≥3" acceptance.
- **Placeholder scan:** Task P is fully concrete (the audit files supply the per-symbol "what's missing"); the schedule names exact sections + counts. No "write tests for the above"-style gaps — the deliverable per row is the named symbol's rustdoc.
- **Dependency consistency:** The A-after-C file list matches C plan's edited files exactly; the A-after-B anchors match B plan's Global-Constraints anchor contract verbatim. The six `→ concept` rows are the only B-coupled work and are explicitly deferred per-row, not per-section.
- **Right-sizing:** A section is the review unit (one `reference/<Section>.md` worth of edits); batches are dispatch/merge units. This honors the spec's "one playbook, not ~80 plans" decision.
