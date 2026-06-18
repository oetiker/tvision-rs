# TV 2.0 Programming Guide — coverage & documentation audit (design)

**Date:** 2026-06-18
**Status:** design approved, awaiting spec review → writing-plans
**Round scope:** **AUDIT ONLY.** This round produces discovery artifacts (a
coverage matrix, a code-gap report, and routed doc scorecards). It makes **no
code changes and no documentation edits.** Fixes are a separately-planned
follow-up driven by the artifacts this round produces.

## Goal

Use the original **Borland *Turbo Vision* Version 2.0 Programming Guide (1992)**
(`Turbo_Vision_Version_2.0_Programming_Guide_1992.pdf`, 640 pp.) as an
independent, authoritative cross-check that the `tvision-rs` port:

1. **covers the full API surface and behavioral possibilities** of the original
   Turbo Vision — nothing was silently dropped; and
2. **is documented to a "what it does + how it is meant to be used" bar**, not
   merely naming fields and methods.

The port was carried out against *magiblot/tvision* (modern C++). The 1992 guide
is a **second, independent source** — it catches anything the magiblot-driven
pass missed and supplies the original intent/usage prose that good docs need.

## The guide's structure (and how it maps to our work)

| Guide part | Pages | Nature | Maps to |
|---|---|---|---|
| Part 1 — *Learning Turbo Vision* (tutorial) | 7–89 | Worked tutorial | **Prose-quality model only** — not separately audited |
| Part 2 — *Using Turbo Vision* (concepts) | 93–314 | Conceptual "how it fits / how to use" | **mdBook developer guide** |
| Part 3 — *Turbo Vision Reference* (alphabetical) | 317–586 | Per-symbol reference: each `T*` class as Fields / Methods / Palette, plus every global var/proc/func/type and constant family | **rustdoc (per-symbol)** |

This division is deliberate and mirrors the user's intent: **concepts belong in
the mdBook; the hardcore per-symbol reference belongs in rustdoc.** The audit
therefore yields two *routed* doc outputs (Part 3 → rustdoc scorecard; Part 2 →
mdBook concept-coverage checklist).

The Part 3 TOC gives an **exact page range for every entry**, so each audit unit
is precisely page-addressable for a subagent brief.

## Two independent axes recorded per reference entry

### Axis 1 — Code coverage (four buckets)

| Bucket | Meaning | Evidence required |
|---|---|---|
| `PORTED` | Direct counterpart exists | the `tv::` symbol path |
| `EQUIVALENT` | Idiomatic Rust analog, different shape (flag word → struct-of-bools; `getData`/`setData` → D10 value protocol; class palette → `Theme`; `infoPtr` → `ViewId`) | the `tv::` analog **+ a one-line mapping** |
| `NOT-PORTED` | Intentionally absent | **a written reason** (DOS-only, Pascal-ism, superseded by a D-rule, memory-manager artifact, …) |
| `MISSING` | Should exist, does not | → a row in the **gap report** (the bucket we are hunting) |

`NOT-PORTED` reasons are recorded in a permanent register so future audits never
re-flag the same intentional omission. This is consistent with the project rule
"docs/comments say ported OR deliberately-not-ported-with-reason, never
'deferred'."

### Axis 1b — Correctness flag (`PORTED`/`EQUIVALENT` entries)

A symbol can be *present but wrong* — the audit must surface missing **and**
incorrect code, not just absences. Every `PORTED`/`EQUIVALENT` entry also carries
a correctness flag:

| Flag | Meaning |
|---|---|
| `OK` | Behavior/signature matches the 1992 spec (or matches it via a **documented** D-rule deviation) |
| `SUSPECT` | Behavior, signature, defaults, or semantics appear to **diverge** from the 1992 spec with **no** documented deviation — a likely port bug → a row in the **gap report** (correctness finding, distinct from `MISSING`) |

A `SUSPECT` flag must cite the specific divergence (wrong default, missing
side-effect, inverted condition, dropped parameter, off-by-one bound, …) and the
guide page that establishes the expected behavior. Deliberate, D-rule-backed
deviations are `OK` — divergence is only `SUSPECT` when it is *undocumented*.

### Axis 2 — Doc quality (rustdoc; `PORTED`/`EQUIVALENT` **public** symbols only)

`pub(crate)`/internal symbols → `N/A` (note visibility). Score against:

| Score | Bar |
|---|---|
| 0 | Undocumented |
| 1 | Restates the signature ("the `foo` field") |
| 2 | Explains **what** it does |
| 3 | **What + how/when to use it** (+ a `# Turbo Vision heritage` section where the C++ lineage aids understanding) — the target |

When the gap is genuinely conceptual (the symbol can't carry it alone — event
phase, Z-order, modal loop, cache buffers), the entry is flagged
**`→ concept`** and routed to the mdBook concept-coverage checklist instead of
demanding a longer rustdoc comment.

## Secondary sweep — Part 2 behavioral capabilities

Part 2 describes *possibilities* that aren't a single named symbol: event
routing & phase, positional/focused/broadcast event flow, masking, Z-order,
modal group execution, cache buffers / locking, idle time, validate-on-
focus-change / on-demand, grow modes, inter-view messaging. A lighter sweep
produces a **capability → present-in-port? → covered-in-mdBook? → where**
checklist. This is the input to the mdBook concept work.

## Artifacts (all under `docs/audit/`)

| File | Contents |
|---|---|
| `docs/audit/reference/<Section>.md` | One file per reference section (a class, or a grouped batch of globals/constants/types). Per-entry table: entry · guide page · Axis-1 bucket · correctness flag · `tv::` symbol/mapping · Axis-2 doc score · notes. |
| `docs/audit/coverage-matrix.md` | Top-level index: one row per section with rolled-up counts (#ported / #equivalent / #not-ported / #missing / #suspect, avg doc score) and a link to its `reference/` file. |
| `docs/audit/gap-report.md` | The actionable **code** backlog, in two parts: (1) **missing** — every `MISSING` entry; (2) **wrong** — every `SUSPECT` correctness finding with its cited divergence + guide page. Plus the permanent `NOT-PORTED`-with-reason register. |
| `docs/audit/rustdoc-scorecard.md` | Every public symbol scoring < 3, with what's missing — input to a later rustdoc pass. |
| `docs/audit/concept-coverage.md` | Part 2 capability checklist + all Axis-2 `→ concept` routes — input to a later mdBook pass. |

(The design doc for *this planning round* lives here, in
`docs/superpowers/specs/`; the audit *outputs* live in `docs/audit/`.)

## Method — subagent-driven, read-only auditors

Follows the project's established subagent-driven methodology, but the
subagents are **read-only auditors**, not implementers. Per audit unit:

1. **Auditor subagent (fresh, isolated, self-contained brief).** Inline: the
   section name, its **exact PDF page range** (from the Part 3 TOC), the Rust
   module path(s) to read, the magiblot C++ path for cross-checking original
   semantics, the Axis-1 taxonomy + Axis-2 rubric verbatim, and the output table
   schema. The auditor reads the PDF pages + the Rust source/rustdoc, classifies
   **every** field/method/palette/global/constant, **flags correctness
   (`OK`/`SUSPECT`) by comparing observed Rust behavior against the guide's
   described behavior**, scores docs, and returns its `reference/<Section>.md`
   markdown. **Model:** Sonnet for mechanical class
   sweeps; the strongest model for the cross-cutting globals/constants and the
   Part 2 sweep.
2. **Orchestrator integrates** the returned markdown into `docs/audit/`, then
   rolls up the coverage matrix and the two scorecards. Auditors are read-only
   and return text, so they **parallelize freely** — no shared-tree writes, no
   worktrees needed (the orchestrator owns all file writes).

## Batching (Part 3 grouped by kin; exact pages enumerated in the plan)

Rough groupings (the writing-plans step pins exact page ranges per the TOC):

1. **Base/primitives** — `TObject`, `TPoint`, `TRect`, `TView`, `TGroup`,
   `TFrame`, `TDrawBuffer`/`TPalette`/`TCharSet` types.
2. **App/desktop/window** — `TProgram`, `TApplication`, `TDesktop`,
   `TBackground`, `TWindow`, `TDialog`.
3. **Controls I** — `TButton`, `TCluster`, `TCheckBoxes`, `TRadioButtons`,
   `TMultiCheckBoxes`, `TInputLine`, `TLabel`, `TStaticText`, `TParamText`.
4. **Controls II / lists** — `TListViewer`, `TListBox`, `TSortedListBox`,
   `TScrollBar`, `TScroller`, `THistory*`.
5. **Menus & status line** — `TMenuView`, `TMenuBar`, `TMenuBox`, `TStatusLine`,
   + their `T*Item`/`T*Def`/`T*Str` types.
6. **Editor & text** — `TEditor`, `TMemo`, `TFileEditor`, `TEditWindow`,
   `TTerminal`, `TTextDevice`, `TIndicator`, edit-buffer types.
7. **Dialogs & files** — `TFileDialog`, `TChDirDialog`, `TFileList`/
   `TFileInfoPane`/`TFileInputLine`, `TColorDialog` + color-* objects/types,
   `TDirListBox`/`TDirCollection`/`TDirEntry`.
8. **Validators** — `TValidator`, `TFilterValidator`, `TRangeValidator`,
   `TLookupValidator`, `TStringLookupValidator`, `TPXPictureValidator`.
9. **Outline** — `TOutlineViewer`, `TOutline`, `TNode`.
10. **Collections & streams** — `TCollection`, `TSorted*`/`TString*` collections,
    `TStream`/`TBufStream`/`TDosStream`/`TEmsStream`/`TResourceFile`/string-list
    machinery (expect many `NOT-PORTED`: DOS/EMS/streamable).
11. **Globals & constant families** — `Application`/`Desktop`/`Clipboard` vars,
    the `*XXXX` constant families (`cmXXXX`, `sfXXXX`, `ofXXXX`, `evXXXX`,
    `kbXXXX`, …), free procedures/functions (`NewItem`, `MessageBox`,
    `InputBox`, `FormatStr`, memory/DOS procs — expect many `NOT-PORTED`).
12. **Part 2 behavioral sweep** — the capability checklist (separate from the
    per-symbol batches).

## Self-verification of the audit itself

- **TOC reconciliation:** after assembly, the coverage-matrix section list is
  diffed against the Part 3 TOC entry list so nothing in pp. 317–586 is silently
  skipped (no-silent-caps).
- **Anti-hallucination spot check:** a fresh reviewer subagent re-checks a
  sample of per-section files — confirms the cited `tv::` symbols actually exist
  in the source and the `MISSING`/`NOT-PORTED`/`SUSPECT` calls hold (especially
  that each `SUSPECT` cites a real divergence and isn't a documented D-rule
  deviation mistaken for a bug) — before the roll-ups are trusted.

## Explicitly out of scope (this round)

- Any code change, new API, or re-port of a `NOT-PORTED` item.
- Any rustdoc or mdBook edit.
- Re-auditing Part 1 (tutorial) at symbol level.
- Scoring mdBook chapters (the concept checklist *names* gaps; it does not grade
  existing chapters).

## Acceptance (this round is done when)

- Every Part 3 reference section has a `docs/audit/reference/<Section>.md` with
  both axes (code bucket + correctness flag + doc score) filled for every entry.
- `coverage-matrix.md` reconciles 1:1 against the Part 3 TOC (no skips).
- `gap-report.md`, `rustdoc-scorecard.md`, and `concept-coverage.md` are
  populated and cross-linked.
- The anti-hallucination spot check passes on the sampled sections.
