# HANDOVER — Audit documentation-backlog closure (resume here)

**Date:** 2026-06-19  **Author of this run:** Opus 4.8 orchestrator session
**Effort:** close the documentation backlog the TV2 coverage audit produced
(`docs/audit/`). This file is the forward-looking resume point for a **new
session**. Read it, then the spec + 3 plans, then the ledger.

---

## 1. What this effort is

The audit (`docs/audit/`) found **0 code gaps** (0 missing, 0 suspect) but a large
**documentation** backlog. Spec + plans:

- **Spec (umbrella):** `docs/superpowers/specs/2026-06-19-audit-doc-backlog-closure-design.md`
- **Plan C** (gate): `docs/superpowers/plans/2026-06-19-c-observation-triage.md`
- **Plan B** (mdBook concept chapters): `docs/superpowers/plans/2026-06-19-b-concept-chapters.md`
- **Plan A** (rustdoc score-3 sweep — a playbook applied per section): `docs/superpowers/plans/2026-06-19-a-rustdoc-sweep.md`

Order is **C → B → A**, but B and A run mostly in parallel (B edits `docs/book/src/*.md`,
A edits rustdoc in `src/*.rs`). The C gate is the only hard prerequisite and **it is DONE**.

## 2. Durable progress ledger (READ THIS FIRST on resume)

`.git/sdd/progress.md` — open it with:
```
cat "$(git rev-parse --git-path sdd)/progress.md"
```
It lists every landed section, the verified checkpoint, and the hard-won lessons.
**Trust the ledger + `git log` over any recollection.** Do not re-do a section it marks complete.

## 3. Branch & current state

- **Integration branch:** `docs/audit-backlog-closure` (off `main` @ `bc15704`). **`main` is untouched.**
- **HEAD:** `b105cf3` (TEditor). 12+ commits.
- **Verified checkpoint:** commit `e2d23f1` was clean-build verified (**fresh** target dir): 1273 tests pass, fmt clean. Everything after `e2d23f1` is doc-only rustdoc + the C-gate code (already verified) — low risk, but **re-verify on a fresh target dir** (see §6) before declaring done.

### C gate — COMPLETE (the code-bearing part)
- `InputLine::set_validator` (extension) — `daee212`
- `Program::set_on_idle` (foundation idle seam, borrow-safe, fires through modal loops) — `fc3313b`+`0a61794`+doc `399e187`
- 7 deliberate-absence doc notes + all 8 audit §2b dispositions — `83cf5d0`

### A sweep — 10 sections landed (each implement → spec-review → quality-review → cherry-pick)
TRect, TPoint, TScrollBar, TButton, TListViewer, TEvent, TOutline, TInputLine, TWindow, TEditor.
(~110 public symbols raised to score-3.)

### IN-FLIGHT loose ends to reconcile FIRST on resume
1. **TMenuItem** — implemented, **NOT reviewed, NOT cherry-picked.** Branch `a/a-menuitem` @ `1bd6bf3`, worktree `/scratch/oetiker/claude-worktrees/rstv-a-menuitem`. Report: `.git/sdd/a-menuitem-report.md` (12 rows → 3, 5 doctests, fmt clean). **Action:** run the quality+spec review (use the review template in §5), then cherry-pick onto the integration branch and `git worktree remove` + `git branch -D`.
2. **TDialog** — implemented, **NOT reviewed, NOT cherry-picked.** Branch `a/a-dialog` @ `cb693d8`, worktree `/scratch/oetiker/claude-worktrees/rstv-a-dialog`. Report: `.git/sdd/a-dialog-report.md`. **Action:** review (spec+quality) then cherry-pick + clean up the worktree. (TDialog.md has 1 `→concept` row that should have been left for B — confirm the implementer deferred it.)

## 4. What remains

- **A sweep:** ~63 more reference sections. The **worklist is `docs/audit/rustdoc-scorecard.md`** (per-section `doc<3` counts) and the per-section `docs/audit/reference/<Section>.md` files (each row's Notes say exactly what reaches score-3). Priority queue (score 0/1) in the scorecard front-loads the highest-impact rows.
- **theme.rs Role pass (one consolidated task):** per-section sweeps **skip `src/theme.rs`** (see §6). One dedicated pass documents ALL remaining `Role::*` variants. Already done inline: ScrollBar(`#Colors`), ListViewer(`ListRoles`), Outline(`Role::Outline*`). Deferred so far: `Role::Button*`, `Role::Input*`, and any others sections flagged.
- **Multi-file sections deferred for careful SEQUENTIAL handling** (they touch shared `program.rs`/`group.rs`/`context.rs` and would conflict with parallel worktrees): **TView** (7 files, 32 rows, has DrawView `→concept`), **TGroup**, **TProgram**. Do these one at a time, not in parallel worktrees.
- **Workstream B (10 chapter tasks):** all unblocked now (C gate done — B Task 2's `git grep set_on_idle` precondition is satisfied). B edits `docs/book/src/*.md`; fully parallel with A. The 6 `→concept` anchors B must create are listed in Plan B's Global Constraints; A's `→concept` rows link to them (do those A rows after the matching B chapter).

## 5. The proven loop (subagent-driven; this is what worked)

Per A section: dispatch a **Sonnet** implementer in its own git worktree (own
`CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target-<tag>`) with the A-playbook
prompt (the section's `docs/audit/reference/<Section>.md` IS its task list) →
on DONE, `scripts/review-package <base> <head>` + dispatch a **Sonnet** task
reviewer (spec compliance + code quality) → fix Important findings (orchestrator
applies one-line doc fixes directly in the worktree + `--amend`; bigger → fix
subagent) → **cherry-pick** the single commit onto the integration branch in the
**main checkout** → `git worktree remove` + `git branch -D` → append to ledger.

- Skill scripts: `/home/oetiker/.claude/plugins/cache/claude-plugins-official/superpowers/6.0.2/skills/subagent-driven-development/scripts/{task-brief,review-package}`
- **Main checkout = merge-only tree.** All implementers run in worktrees so the
  orchestrator can cherry-pick without colliding with an active writer.
- Cap: **≤2 building agents** (shared 128-core box, 4-core budget; `-j 2` + `--test-threads=2`).
- C-gate code uses **Opus** for implementer+reviewer; mechanical rustdoc uses **Sonnet**.
- A-playbook prompt template + the standing policies: see any recent A dispatch in this
  session's transcript, or reconstruct from Plan A's "Task P" + §6 below.

## 6. CRITICAL GOTCHAS (these will bite you)

1. **Shared target-dir corruption → PHANTOM build errors.** The canonical
   `/home/oetiker/scratch/cargo-target` gets corrupted by overlapping worktree +
   verification builds, producing **phantom** errors like `#[delegate]: skip(apply_list_scroll)
   is not a method of trait View`, `apply_scroll_sync is not a member of trait View`,
   `Window/Desktop/Dialog: View is not satisfied`. These are NOT real. For any
   **authoritative integrated verification, use a FRESH target dir** (e.g.
   `CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target-verify cargo test --workspace`).
   Worktree agents already use own per-tag dirs (reliable).
2. **`<new-diagnostics>` blocks are the SAME stale-macro noise.** The IDE/diagnostic
   engine runs against a stale `tvision-rs-macros`. **Trust cargo, not diagnostics.**
   (Matches the `diagnostics-trust-cargo` / `trust-cargo-not-diagnostics` memories.)
3. **theme.rs policy:** per-section A sweeps **must skip `src/theme.rs`** Role rows
   (leave them below-bar, note "deferred to theme pass"). Otherwise parallel worktrees
   conflict on `theme.rs`. One consolidated theme-Role pass at the end.
4. **`→concept` rows:** leave them; they link into B chapters (a separate workstream).
5. **Multi-file sections (TView/TGroup/TProgram):** run SEQUENTIALLY, not in parallel
   worktrees (shared `program.rs`/`group.rs`/`context.rs`).
6. Some audit rows can only reach 3 via a **code change** (e.g. TWindow `Close`,
   no public `close()`); those correctly stay below-bar — note, don't force.
7. **Worktrees** live under `/scratch/oetiker/claude-worktrees/rstv-<tag>`; run
   `git merge`/`cherry-pick` only in `/home/oetiker/checkouts/rstv`, never inside a worktree.

## 7. Finishing (when the sweep + theme pass + B are done)

- Fresh-target-dir gate: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`, `cargo build --examples`, `cargo xtask test`, `cargo xtask docs` (book↔api link check — this is where any broken `→concept`/intra-doc link surfaces).
- Reconcile `docs/audit/rustdoc-scorecard.md` "Total below bar" down to only intentionally-private rows; update `coverage-matrix.md` `doc<3` column; confirm no silent skips.
- Final whole-branch review (Opus) per `superpowers:requesting-code-review`, triaging the Minor findings logged in the ledger.
- Add an `IMPLEMENTATION-LOG.md` section; update `docs/HANDOVER.md`; then merge `docs/audit-backlog-closure` → `main` (fast-forward if possible) via `superpowers:finishing-a-development-branch`.
- Delete this `HANDOVER-doc.md` once the work lands.
