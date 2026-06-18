# TV 2.0 Programming Guide — coverage & documentation audit

Read-only cross-check of `tvision-rs` against the original **Borland *Turbo
Vision* Version 2.0 Programming Guide (1992)**. This round produces discovery
artifacts only — **no code, rustdoc, or mdBook changes.**

- Design spec: [`../superpowers/specs/2026-06-18-tv2-guide-coverage-audit-design.md`](../superpowers/specs/2026-06-18-tv2-guide-coverage-audit-design.md)
- Plan: [`../superpowers/plans/2026-06-18-tv2-guide-coverage-audit.md`](../superpowers/plans/2026-06-18-tv2-guide-coverage-audit.md)

## Artifacts

| File | Contents |
|---|---|
| `reference/<Section>.md` | Per reference section: every field/method/palette/global/constant classified on all three axes. |
| `coverage-matrix.md` | Top-level index + rolled-up counts per section; links to everything. |
| `gap-report.md` | Actionable code backlog: **missing** + **wrong** findings; plus the permanent NOT-PORTED register. |
| `rustdoc-scorecard.md` | Every public symbol scoring < 3 on docs, with what's missing. |
| `concept-coverage.md` | Part 2 behavioral-capability checklist + all `→ concept` doc routes. |

## The three axes (recorded for every entry)

### Axis 1 — code coverage bucket

| Bucket | Meaning | Evidence |
|---|---|---|
| `PORTED` | direct counterpart | the `tv::` symbol path |
| `EQUIVALENT` | idiomatic analog, different shape | the `tv::` analog **+ one-line mapping** |
| `NOT-PORTED` | intentionally absent | **a written reason** |
| `MISSING` | should exist, does not | → gap-report |
| `UNSURE` | auditor could not classify | **a question** (never omit the row) |

### Axis 1b — correctness flag (`PORTED`/`EQUIVALENT` only)

| Flag | Meaning |
|---|---|
| `OK` | matches the 1992 spec (or matches via a **documented** D-rule deviation) |
| `SUSPECT` | undocumented divergence (wrong default / signature / side-effect / bound / inverted condition) — **cite the divergence + guide page** |

A deliberate, commented deviation is `OK`, not `SUSPECT`.

### Axis 2 — rustdoc score (`PORTED`/`EQUIVALENT` **public** symbols; else `N/A`)

| Score | Bar |
|---|---|
| 0 | undocumented |
| 1 | restates the signature ("the `foo` field") |
| 2 | explains **what** it does |
| 3 | **what + how/when to use it** (+ heritage section where it helps) — target |

If the gap is genuinely conceptual (the symbol can't carry it — event phase,
Z-order, modal loop, cache buffers), add `→ concept`: it routes to the mdBook
guide, not a longer rustdoc comment.

## Per-section row schema

```markdown
# <Section>  (guide pp. <start>–<end>)

Rust module(s): <path(s)>   |   magiblot: <header/impl>

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `dataSize` (method) | 421 | PORTED | OK | `tv::Editor::data_size` | 2 | missing "how to use" |
| `bufSize` (field) | 422 | EQUIVALENT | OK | `Editor.buffer: Vec<u8>` (len) | N/A | private |
| `TEmsStream` | 432 | NOT-PORTED | — | — | — | DOS EMS; no analog (reason) |
| `someProc` | 999 | MISSING | — | — | — | no counterpart found |
```

## Known idiomatic mappings (treat as `EQUIVALENT`, never `MISSING`/`SUSPECT`)

- flag word → struct-of-bools
- `getData`/`setData` → the D10 value protocol (`src/data.rs`)
- a class `Palette` → `tv::Theme` (`src/theme.rs`)
- `infoPtr` / raw pointers → `ViewId` handles
- `TStreamable` / streams → dropped (serde-if-revived)
- DOS / EMS / memory-manager machinery → no analog (DOS-era)
- `TCollection` family → idiomatic Rust `Vec` / slices

## Method note

Auditors are **read-only**. Each writes its own `reference/<Section>.md` (distinct
path → parallel-safe) and returns only status + counts. Roll-ups
(`coverage-matrix`, `gap-report`, `rustdoc-scorecard`) are assembled by the
orchestrator in a later pass.

## Source baseline & the private-symbol re-check

This audit branch is based on **`main`** (audit-only history, no `src/` changes).
The audit itself was originally conducted against a working tree that *also*
included the in-flight **`consumer-api-coverage`** work (public window/dialog
decoration setters, `Group::get_help_ctx` bubbling, a `program.rs` help-ctx
arm). A follow-up **private-symbol re-check** (a verification fleet over all 109
files) then re-verified **every** cited Rust symbol against this branch's
`main`-based `src/` and corrected all that didn't resolve:

- **28 citations corrected** across 7 files; **0 left unverified** — every cited
  symbol now resolves against this branch's `src/`. Each fix carries a
  `CORRECTED (private-symbol re-check): was \`<old>\`` note in its row.
- Genuine hallucinations caught (existed in **no** tree): e.g. `Modifiers.dim`
  (→ `blink`/`strike`/`no_shadow`), `OutlineFlags::expanded`/`NodeState`
  (→ the private `OV_*` consts).
- Baseline-shift fixes (frontier symbol → its `main` form, since this branch is
  `main`): `Group::get_help_ctx` override → the `View` default + `program.rs`
  aggregation; `Dialog::set_palette`/`with_palette` → `pub(crate)
  Window::set_palette`; plus `program.rs` line-number drift from the unmerged
  help-ctx commit.

**Confidence:** buckets, classifications, and the gap-report (missing/wrong) were
adversarially spot-checked and hold; after the re-check, the per-symbol *name and
visibility* citations also resolve against this branch. A few rows that describe
the richer `consumer-api-coverage` forms will read more naturally once that work
merges to `main`.
