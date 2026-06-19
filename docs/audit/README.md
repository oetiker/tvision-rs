# TV 2.0 Programming Guide — coverage & documentation audit

A coverage & documentation cross-check of `tvision-rs` against the original
**Borland *Turbo Vision* Version 2.0 Programming Guide (1992)**: every documented
API entry classified by how it maps to the Rust port, plus the resulting code and
documentation gaps.

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

## Baseline

Citations resolve against the `tvision-rs` source on `main`. A few entries
describe capability that the in-flight `consumer-api-coverage` work refines
(public window/dialog decoration setters, `Group::get_help_ctx` bubbling, a
`program.rs` help-context arm); those rows describe the `main` form and will read
more naturally once that work merges.
