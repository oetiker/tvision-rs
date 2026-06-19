# HANDOVER — Audit doc-backlog closure: **ALMOST DONE, finish + merge** (resume here)

**Date:** 2026-06-19 (cont.)  **Author:** Opus 4.8 orchestrator session
**State:** the entire sweep is **content-complete and green**. Only the final
review verdict, the merge to `main`, and deleting this file remain. This is a
**short** finish — do NOT re-run the sweep.

---

## 0. TL;DR — what's left (≈2 steps — the review is DONE)
1. **Merge** `docs/audit-backlog-closure` → `main` (**fast-forward is possible** —
   main is still at `bc15704`, the branch point). Use
   `superpowers:finishing-a-development-branch`.
2. **Delete `HANDOVER-doc.md`** (this file) and commit, as the last step.

The final whole-branch review is **complete** — see §3. Verdict
**APPROVE-WITH-MINORS**; all findings were fixed in commit `5d29db1`. Nothing else
is pending; the branch is merge-ready.

## 1. Branch & state
- **Branch:** `docs/audit-backlog-closure`. **~58 commits** since `main` (@ `bc15704`);
  HEAD is the final-review-fix `5d29db1` + this handover commit on top. **`main`
  untouched — FF-mergeable** (verify: `git merge-base main HEAD` == `git rev-parse main`).
- **Durable ledger:** `cat "$(git rev-parse --git-path sdd)/progress.md"` — every
  landed section + lessons. Trust ledger + `git log` over recollection.
- **Outcome:** below-bar public symbols **644 → 3**. The 3 remaining are
  structurally blocked (no rustdoc-only fix), and are CORRECT to leave:
  - `TIndicator` **SetState** — no `set_state` override exists (code change).
  - `TWindow` **Close** — no public `close()`; logic in `handle_event` (code change).
  - `TTextDevice` **GetPalette** — `→concept` row, no public Rust symbol.

## 2. ALL gates GREEN (verified at HEAD `383dfb1`, fresh-ish target dir `/home/oetiker/scratch/cargo-target-finalgate`)
- `cargo test --workspace -j2 -- --test-threads=2` → **1323 passed, 0 failed**
- `cargo clippy --workspace --all-targets -j2 -- -D warnings` → clean
- `cargo fmt --all --check` → clean
- `cargo build --examples -j2` → clean
- `cargo xtask test` → **OK guide doctests (35 chapters)**
- `cargo xtask docs` → **OK: integrated site** (book↔api link check passes; exit 0)

The later commits after the last full run are doc/md only — if you want belt-and-
braces, re-run `cargo test --workspace` + `cargo xtask docs` once on a FRESH target
dir before merging.

## 3. The final review — DONE (verdict APPROVE-WITH-MINORS, all fixed)
A Sonnet "final branch honesty review" audited ~120 rows across 10 sections, 6
concept anchors, ~15 intra-doc links, and scanned all 42 changed `.rs` files for
non-doc code changes. Verdict **APPROVE-WITH-MINORS**. All findings fixed in
`5d29db1`:
- **Important (fixed):** `StringLookupValidator::new_string_list` (pub,
  `src/validate.rs`) was genuinely score-2 while its section summary/scorecard
  counted it as 0 — the true below-bar was 4. Documented to score-3 (runtime re-list
  use case + nil-arg heritage note); audit row + count now honest.
- **Minor (fixed):** 5 stale score-2 audit *table cells* (TMenuView GetHelpCtx,
  TValidator is_status_ok, TLabel CLabel, Globals-363-378 PositionalEvents +
  ShadowAttr) whose cited symbols are genuinely score-3 in code (summaries already
  said 0) — cells bumped to match.
- **Confirmed safe:** the `Program::pump_once` `()`→`bool` change is the intended
  C-gate (callers discard the bool; `Application::pump_once` still returns `()`).
  The only non-doc code in the branch is the C-gate (set_on_idle / pump_and_drive /
  IdleHook in program.rs; set_validator in input_line.rs). All 6 `→concept` anchors
  exist with substantive content; sampled intra-doc links all resolve to public
  symbols; the 3 remaining below-bar rows are correctly code-change/concept-blocked.

Below-bar is now a **genuine 3**. Nothing else to verify — proceed to merge.

## 4. What was done (so you don't redo it)
- **C gate (code):** `InputLine::set_validator` + `Program::set_on_idle` (idle seam,
  `pump_and_drive`) — the ONLY behaviour changes; both reviewed. Plus 7 deliberate-
  absence notes.
- **A sweep (37 `docs(rustdoc)` commits):** every audit section raised to score-3 or
  honest N/A; consolidated `src/theme.rs` Role pass (~75 variants + WindowPalette→Role
  table); reconciliation pass closed TCommandSet (genuine gap) + colorpick + cross-refs.
- **B (9 `docs(guide)` commits):** all 10 concept-chapter tasks; the **6 `→concept`
  anchors exist** and A links to them (`#the-phase-field`, `#ending-a-modal-execview`,
  `#the-modal-loop-execute`, `#endmodal`, `#draw-on-demand-vs-whole-tree`,
  `#validator-error-dialogs`).
- **Scorecard + coverage-matrix reconciled** (`67c17e3`): headline 644→3, matrix
  `doc<3` column regenerated from per-section rollups.
- **IMPLEMENTATION-LOG** entry written (`383dfb1`).

## 5. Non-obvious gotchas / decisions (don't be surprised)
1. **Pre-existing book-link bug fixed (`2ac7a70`):** the `rstv→tvision-rs` rename left
   **806 site-wide book links** at `api/tvision-rs/` (hyphen); rustdoc emits
   `api/tvision_rs/` (underscore). `main` failed `xtask docs` identically. Fixed across
   33 chapters. This is the reason `xtask docs` now passes.
2. **`docs/HANDOVER.md` has a STRAY uncommitted modification** (from the *other*
   `consumer-api-coverage` effort, present since before this session). It is **NOT
   mine** — leave it uncommitted/untouched; do not `git add` it into the merge, do not
   revert it. The IMPLEMENTATION-LOG (not docs/HANDOVER.md) is this effort's record.
3. **Pre-existing base-tree debt (out of scope, noted in the LOG):** a handful of
   `(deviation Dx)` porting labels still live in module/struct heritage docs
   (color.rs, theme.rs, view.rs, un-rewritten parts of event/menu/window). A future
   site-wide bookkeeping-strip pass should remove them. Do NOT block the merge on this.
4. **`<new-diagnostics>` blocks = stale-macro phantom noise** (IDE runs vs stale
   `tvision-rs-macros`). Trust cargo. (Matches `diagnostics-trust-cargo` memory.)
5. **The recurring sweep defect was bad intra-doc links** (public→`pub(crate)`, non-
   existent symbols like `Group::insert_child`/`Context::make_local`, private
   `FileList::search`) and leaked `deviation Dx` labels — all caught + fixed before
   merge by grepping every link target's visibility. If you add anything, do the same.
6. All worktrees/branches from the sweep are **removed**. Merge/cherry-pick only in
   `/home/oetiker/checkouts/rstv`.

## 6. Finish recipe (the actual commands)
```
cd /home/oetiker/checkouts/rstv
# (optional belt-and-braces re-verify on a fresh dir)
CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target-verify cargo test --workspace -j2 -- --test-threads=2
CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target-verify cargo xtask docs
# get/redo the final review (see §3); then:
git rm HANDOVER-doc.md && git commit -m "docs: remove finished-effort resume file"
# fast-forward merge (main untouched):
git checkout main && git merge --ff-only docs/audit-backlog-closure
# (then per finishing-a-development-branch: optionally delete the branch)
```
Leave the stray `docs/HANDOVER.md` working-tree change alone throughout.
