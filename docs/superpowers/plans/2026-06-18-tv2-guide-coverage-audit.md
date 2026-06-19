# TV 2.0 Guide Coverage & Documentation Audit — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Cross-check `tvision-rs` against the original 1992 *Turbo Vision 2.0
Programming Guide* and produce read-only audit artifacts that surface **missing**
code, **wrong** code, and **under-documented** API — without changing any code or
docs this round.

**Architecture:** Read-only auditor subagents, one per reference section,
page-addressed from the guide's TOC. Each classifies every entry on three axes
(code-coverage bucket · correctness flag · rustdoc score), comparing the guide's
described behavior against the Rust source. The orchestrator owns all file writes
under `docs/audit/`, rolls up the matrix and reports, then a fresh reviewer
verifies a sample and reconciles against the TOC.

**Tech Stack:** Markdown artifacts only. PDF read via the Read tool's `pages`
param. Source read via Read/Grep/Glob. No build, no tests-as-code — verification
is reconciliation + spot check.

**Design spec:** [`docs/superpowers/specs/2026-06-18-tv2-guide-coverage-audit-design.md`](file:///home/oetiker/checkouts/rstv/docs/superpowers/specs/2026-06-18-tv2-guide-coverage-audit-design.md)

## Global Constraints

- **AUDIT ONLY.** No `src/` edits, no rustdoc edits, no mdBook edits, no re-port
  of any `NOT-PORTED` item. The only files created are under `docs/audit/`.
- **Branch first.** We are on `main`; do all work on a branch
  `audit/tv2-guide-coverage`. Do not commit audit artifacts to `main` directly.
- **PDF:** `/home/oetiker/checkouts/rstv/Turbo_Vision_Version_2.0_Programming_Guide_1992.pdf`.
  Read only the page range a task names; max 20 pages per Read call.
- **Original C++ for semantics cross-check:**
  `/home/oetiker/scratch/tvision-spec/magiblot-tvision/` (headers
  `include/tvision/`, impl `source/tvision/`). The *guide* establishes intended
  behavior; magiblot shows the concrete algorithm the port followed.
- **Rust source root:** `src/` (layout is mapped per task). `CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target`
  if any command is ever run (none required this round).
- **Three axes, recorded for every entry** (verbatim from the spec):
  - *Axis 1 — code bucket:* `PORTED` (name the `tv::` symbol) · `EQUIVALENT`
    (idiomatic analog + one-line mapping) · `NOT-PORTED` (written reason) ·
    `MISSING` (gap → gap-report).
  - *Axis 1b — correctness flag* (`PORTED`/`EQUIVALENT` only): `OK` ·
    `SUSPECT` (undocumented divergence — cite the divergence + guide page).
  - *Axis 2 — rustdoc score* (`PORTED`/`EQUIVALENT` **public** only; else `N/A`):
    `0` undocumented · `1` restates signature · `2` what · `3` what + how/when
    (+ heritage). Conceptual gaps the symbol can't carry → flag `→ concept`.
- **No silent skips.** Every field/method/palette/global/constant the guide lists
  gets a row. If an auditor can't classify one, it writes the row as `UNSURE`
  with a question — never omits it.

---

## Task 0: Scaffold `docs/audit/` + the shared auditor instrument

**Files:**
- Create: `docs/audit/README.md` (taxonomy + rubric reference card — the single
  source every auditor brief quotes)
- Create: `docs/audit/AUDITOR-BRIEF-TEMPLATE.md` (the fill-in-the-blanks dispatch
  brief)
- Create: `docs/audit/reference/.gitkeep`
- Create skeletons (headers only, tables empty): `docs/audit/coverage-matrix.md`,
  `docs/audit/gap-report.md`, `docs/audit/rustdoc-scorecard.md`,
  `docs/audit/concept-coverage.md`

**Interfaces:**
- Produces: the **per-section row schema** every later task emits, and the
  **brief template** every dispatch quotes. Pasted here so later tasks don't
  redefine it.

Per-section file (`docs/audit/reference/<Section>.md`) row schema:

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

Auditor brief template (`AUDITOR-BRIEF-TEMPLATE.md`):

```
You are a READ-ONLY auditor. Do not edit any file. Return ONLY the markdown
for docs/audit/reference/<Section>.md, nothing else.

Section(s): <names>
Guide pages: <range>  (Read the PDF in <=20-page slices)
Rust module(s) to read: <paths>   (also rg/glob to confirm — the hint may be incomplete)
magiblot reference: <header(s)>

For EVERY field, method, palette entry (and every global/constant/type in the
range), emit one table row using this schema: <paste schema>.

Axis 1 (bucket): PORTED | EQUIVALENT | NOT-PORTED(reason) | MISSING. EQUIVALENT
needs a one-line mapping; NOT-PORTED needs a written reason; MISSING means you
searched src/ and found no counterpart.
Axis 1b (corr, PORTED/EQUIVALENT only): OK | SUSPECT. SUSPECT = behavior /
defaults / signature / side-effects diverge from the guide with NO documented
D-rule reason. Cite the divergence + guide page. A deliberate, commented
deviation is OK, not SUSPECT.
Axis 2 (doc, public PORTED/EQUIVALENT only; else N/A): 0/1/2/3 per the rubric;
add → concept if the gap is conceptual (belongs in the mdBook, not the symbol).
Never omit an entry. If unsure, write the row as UNSURE with your question.
Known idiomatic mappings (treat as EQUIVALENT, not MISSING/SUSPECT): flag word →
struct-of-bools; getData/setData → tv::data value protocol (src/data.rs);
class Palette → tv::Theme (src/theme.rs); infoPtr/pointers → ViewId handles;
TStreamable/streams → dropped (serde-if-revived); DOS/EMS/memory-manager → no analog.
```

- [ ] **Step 1:** Create the branch.

```bash
git -C /home/oetiker/checkouts/rstv checkout -b audit/tv2-guide-coverage
```

- [ ] **Step 2:** Write `docs/audit/README.md` containing the verbatim taxonomy,
  the rubric, the row schema (above), and the known-idiomatic-mappings list.
- [ ] **Step 3:** Write `docs/audit/AUDITOR-BRIEF-TEMPLATE.md` (above).
- [ ] **Step 4:** Write the four roll-up skeletons with their headers and empty
  tables, and `docs/audit/reference/.gitkeep`.
- [ ] **Step 5:** Verify the skeleton exists and is internally linked.

```bash
ls docs/audit docs/audit/reference && grep -l "Bucket" docs/audit/README.md
```
Expected: all files listed; README contains the schema.

- [ ] **Step 6:** Commit.

```bash
git add docs/audit && git commit -m "docs(audit): scaffold TV2 guide coverage audit instrument"
```

---

## Per-section page map (shared appendix — batch tasks reference these rows)

Page ranges are from the Part 3 TOC (end = next entry's start). Module hints are
starting points; auditors confirm via `rg`/glob.

| Section | Pages | Rust module hint |
|---|---|---|
| TObject | 488–489 | (no root object — EQUIVALENT/NOT-PORTED) |
| TPoint | 501 | `src/view/geometry.rs` |
| TRect | 518–519 | `src/view/geometry.rs` |
| TView | 560–576 | `src/view/view.rs`, `src/view/mod.rs` |
| TGroup | 445–455 | `src/view/group.rs` |
| TFrame | 443–445 | `src/frame.rs` |
| TDrawBuffer type | 420–421 | `src/screen/draw_buffer.rs` |
| TPalette type | 498–499 | `src/theme.rs` |
| TCharSet / TByteArray / TWordArray / TVideoBuf types | 390–391, 581–582, 560 | `src/screen/`, `src/text.rs` |
| TEvent type | 434–435 | `src/event/mod.rs`, `src/event/key.rs` |
| TProgram | 502–512 | `src/app/program.rs` |
| TApplication | 379–382 | `src/app/application.rs` |
| TDesktop | 412–415 | `src/desktop/desktop.rs` |
| TBackground | 382–383 | `src/desktop/background.rs` |
| TWindow | 577–581 | `src/window/window.rs` |
| TDialog | 415–418 | `src/dialog/dialog.rs` |
| TTitleStr type | 557 | (string alias) |
| TButton | 386–390 | `src/widgets/button.rs` |
| TCluster | 395–400 | `src/widgets/cluster.rs` |
| TCheckBoxes | 393–395 | `src/widgets/cluster.rs` |
| TRadioButtons | 514–516 | `src/widgets/cluster.rs` |
| TMultiCheckBoxes | 486–488 | `src/widgets/cluster.rs` |
| TInputLine | 460–464 | `src/widgets/input_line.rs` |
| TLabel | 465–467 | `src/widgets/` (glob: label/static_text/cluster) |
| TStaticText | 536–537 | `src/widgets/static_text.rs` |
| TParamText | 499–501 | `src/widgets/static_text.rs` |
| TItemList type | 464 | `src/widgets/cluster.rs` |
| TListViewer | 470–474 | `src/widgets/list_viewer.rs` |
| TListBox | 467–470 | `src/widgets/list_box.rs` |
| TSortedListBox | 534–536 | `src/widgets/list_box.rs` |
| TScrollBar | 523–527 | `src/widgets/scrollbar.rs` |
| TScrollChars type | 527 | `src/widgets/scrollbar.rs` |
| TScroller | 527–530 | `src/widgets/scroller.rs` |
| TIndicator | 458–460 | `src/widgets/indicator.rs` |
| THistory | 455–457 | `src/widgets/history.rs` |
| THistoryViewer | 457 | `src/widgets/history.rs` |
| THistoryWindow | 457–458 | `src/widgets/history.rs` |
| TMenuView | 482–485 | `src/menu/menu_view.rs` |
| TMenuBar | 478–480 | `src/menu/menu_bar.rs` |
| TMenuBox | 480–481 | `src/menu/menu_box.rs` |
| TMenu / TMenuItem / TMenuStr types | 477–478, 481, 482 | `src/menu/mod.rs`, `src/menu/menu_session.rs` |
| TStatusLine | 539–542 | `src/status/status_line.rs` |
| TStatusDef / TStatusItem types | 537–539 | `src/status/mod.rs` |
| TEditor | 421–430 | `src/widgets/editor.rs` |
| TEditBuffer type | 421 | `src/widgets/editor.rs` |
| TMemo | 475–477 | `src/widgets/editor.rs` |
| TMemoData type | 477 | `src/widgets/editor.rs` |
| TFileEditor | 438–441 | `src/widgets/editor.rs` |
| TEditWindow | 430–432 | `src/widgets/editor.rs`, `src/window/` |
| TTerminal | 553–555 | `src/widgets/terminal.rs` |
| TTerminalBuffer type | 555–556 | `src/widgets/terminal.rs` |
| TTextDevice | 556–557 | `src/widgets/terminal.rs` |
| TFileDialog | 435–438 | `src/dialog/filedlg.rs` |
| TChDirDialog | 391–393 | `src/dialog/filedlg.rs` |
| TFileList / TFileInfoPane / TFileInputLine | 441 | `src/dialog/filedlg.rs` |
| TDirListBox / TDirCollection / TDirEntry | 418–419 | `src/dialog/filedlg.rs` |
| TFileCollection / TSearchRec / TFindDialogRec / TReplaceDialogRec | 435, 530, 443, 519 | `src/dialog/filedlg.rs`, `src/dialog/msgbox.rs` |
| TColorDialog | 406–409 | `src/dialog/colorpick/mod.rs` |
| TColorDisplay / TColorGroup(List) / TColorIndex / TColorItem(List) / TColorSel / TColorSelector / TMonoSelector | 409–411, 485–486 | `src/dialog/colorpick/*` |
| TValidator | 557–560 | `src/validate.rs` |
| TFilterValidator | 441–443 | `src/validate.rs` |
| TRangeValidator | 516–518 | `src/validate.rs` |
| TLookupValidator | 474–475 | `src/validate.rs` |
| TStringLookupValidator | 551–553 | `src/validate.rs` |
| TPXPictureValidator / TPicResult / TVTransfer | 512–514, 501–502, 576–577 | `src/validate.rs`, `src/data.rs` |
| TOutlineViewer | 491–498 | `src/widgets/outline.rs` |
| TOutline | 489–491 | `src/widgets/outline.rs` |
| TNode type | 488 | `src/widgets/outline.rs` |
| TCollection / TSortedCollection / TStringCollection | 400–406, 531–534, 547–548 | (Rust `Vec`/idiomatic — EQUIVALENT) |
| TStringList / TStrListMaker / TStrIndex(Rec) | 548–550, 546–547 | (resources — likely NOT-PORTED) |
| TStream / TBufStream / TDosStream / TEmsStream / TStreamRec | 542–546, 383–386, 419–420, 432–434 | (streams — likely NOT-PORTED) |
| TResourceFile / TResourceCollection | 519–523 | (resources — likely NOT-PORTED) |
| TCommandSet type | 411–412 | `src/command.rs` |
| TSysErrorFunc / TSItem types | 530–531, 550–551 | `src/app/`, `src/widgets/cluster.rs` |
| Globals & constant families A–S | 317–378 | spread (see Task 11) |
| Constants & misc T–W | 582–586 | spread (see Task 11) |

---

## Tasks 1–11: Part 3 reference batches

**Each of these tasks has the identical shape** (only the section list differs).
The shape, stated once:

> 1. For each section in the batch, dispatch a fresh **read-only auditor**
>    subagent (Sonnet for plain widget/class sections; strongest model for
>    TView/TGroup/TProgram, validators, streams, and the globals batch). Fill the
>    `AUDITOR-BRIEF-TEMPLATE.md` with the section's pages + module hint from the
>    appendix. Auditors are read-only → **dispatch the batch's sections in
>    parallel.**
> 2. **Orchestrator integrates:** write each returned markdown to
>    `docs/audit/reference/<Section>.md` verbatim, then `git diff`-review nothing
>    (no source touched) and add a one-row summary to `coverage-matrix.md`.
> 3. **Verify (the task's "test"):** every section in the batch has a file; every
>    file's table has ≥1 row per Field/Method/Palette the guide TOC lists for it;
>    no `UNSURE` rows left unresolved (resolve by re-reading source or escalating
>    to the user, never by deleting the row).
> 4. **Commit** the batch's reference files + matrix rows.

### Task 1: Batch A — base & primitives
Sections: TObject, TPoint, TRect, TView, TGroup, TFrame, TDrawBuffer, TPalette,
TCharSet/TByteArray/TWordArray/TVideoBuf, TEvent.

- [ ] Dispatch auditors (parallel) per the shape above, pages+hints from the appendix.
- [ ] Integrate returned files into `docs/audit/reference/`.
- [ ] Add matrix rows; verify every section file has full Field/Method/Palette coverage.
- [ ] Verify: `ls docs/audit/reference/ | grep -E 'TView|TGroup|TFrame|TRect|TPoint|TObject|TDrawBuffer|TPalette|TEvent'` shows all.
- [ ] Commit: `git commit -m "docs(audit): batch A — base & primitives"`

### Task 2: Batch B — application, desktop, window
Sections: TProgram, TApplication, TDesktop, TBackground, TWindow, TDialog, TTitleStr.
- [ ] Dispatch (TProgram/TApplication → strongest model; rest Sonnet), integrate, matrix rows.
- [ ] Verify all 7 files present with full coverage.
- [ ] Commit: `git commit -m "docs(audit): batch B — application/desktop/window"`

### Task 3: Batch C — controls I
Sections: TButton, TCluster, TCheckBoxes, TRadioButtons, TMultiCheckBoxes,
TInputLine, TLabel, TStaticText, TParamText, TItemList.
- [ ] Dispatch (parallel), integrate, matrix rows.
- [ ] Verify all 10 files present; confirm cluster family mappings are EQUIVALENT not MISSING.
- [ ] Commit: `git commit -m "docs(audit): batch C — controls I"`

### Task 4: Batch D — lists, scrollers, history
Sections: TListViewer, TListBox, TSortedListBox, TScrollBar, TScrollChars,
TScroller, TIndicator, THistory, THistoryViewer, THistoryWindow.
- [ ] Dispatch, integrate, matrix rows.
- [ ] Verify all 10 files; confirm scroller↔scrollbar broker seam noted where relevant.
- [ ] Commit: `git commit -m "docs(audit): batch D — lists/scrollers/history"`

### Task 5: Batch E — menus & status line
Sections: TMenuView, TMenuBar, TMenuBox, TMenu/TMenuItem/TMenuStr, TStatusLine,
TStatusDef/TStatusItem.
- [ ] Dispatch, integrate, matrix rows.
- [ ] Verify all files present.
- [ ] Commit: `git commit -m "docs(audit): batch E — menus & status line"`

### Task 6: Batch F — editor & text
Sections: TEditor, TEditBuffer, TMemo, TMemoData, TFileEditor, TEditWindow,
TTerminal, TTerminalBuffer, TTextDevice.
- [ ] Dispatch (TEditor → strongest model — largest surface), integrate, matrix rows.
- [ ] Verify all 9 files; confirm clipboard/undo/buffer entries classified.
- [ ] Commit: `git commit -m "docs(audit): batch F — editor & text"`

### Task 7: Batch G — dialogs, files, color
Sections: TFileDialog, TChDirDialog, TFileList/TFileInfoPane/TFileInputLine,
TDirListBox/TDirCollection/TDirEntry, TFileCollection/TSearchRec/TFindDialogRec/
TReplaceDialogRec, TColorDialog, TColorDisplay/TColorGroup(List)/TColorIndex/
TColorItem(List)/TColorSel/TColorSelector/TMonoSelector.
- [ ] Dispatch, integrate, matrix rows.
- [ ] Verify all files; color-* sections map to `src/dialog/colorpick/*`.
- [ ] Commit: `git commit -m "docs(audit): batch G — dialogs/files/color"`

### Task 8: Batch H — validators
Sections: TValidator, TFilterValidator, TRangeValidator, TLookupValidator,
TStringLookupValidator, TPXPictureValidator, TPicResult, TVTransfer.
- [ ] Dispatch (strongest model — semantics-heavy), integrate, matrix rows.
- [ ] Verify all 8 files; note the RegexValidator extension lives alongside the picture-mask port.
- [ ] Commit: `git commit -m "docs(audit): batch H — validators"`

### Task 9: Batch I — outline
Sections: TOutlineViewer, TOutline, TNode.
- [ ] Dispatch, integrate, matrix rows.
- [ ] Verify all 3 files.
- [ ] Commit: `git commit -m "docs(audit): batch I — outline"`

### Task 10: Batch J — collections, streams, resources
Sections: TCollection, TSortedCollection, TStringCollection, TStringList,
TStrListMaker, TStrIndex(Rec), TStream, TBufStream, TDosStream, TEmsStream,
TStreamRec, TResourceFile, TResourceCollection, TCommandSet, TSysErrorFunc, TSItem.
- [ ] Dispatch (strongest model — most NOT-PORTED reasons to write carefully), integrate, matrix rows.
- [ ] Verify all files; every NOT-PORTED (DOS/EMS/streamable/resource) carries a written reason.
- [ ] Commit: `git commit -m "docs(audit): batch J — collections/streams/resources"`

### Task 11: Batch K — globals, free routines, constant families
Range: pp. 317–378 (A–S) + 582–586 (T–W). Chunk into 20-page Read slices.
Sub-units to dispatch (parallel), each "enumerate EVERY variable/proc/func/type/
constant-family in this slice and classify":
- 317–330, 331–346, 347–362, 363–378, 582–586.
Module hints: `src/command.rs` (cmXXXX), `src/event/` (evXXXX/kbXXXX/mbXXXX),
`src/view/view.rs` (sfXXXX/ofXXXX/gfXXXX), `src/window/` (wfXXXX/wpXXXX),
`src/theme.rs` + `src/color.rs` (cXXXX/apXXXX/coXXXX), `src/dialog/msgbox.rs`
(mfXXXX, MessageBox/InputBox), `src/data.rs`, plus globals
`Application`/`Desktop`/`Clipboard`.
- [ ] Dispatch 5 slice-auditors (strongest model), integrate into one or a few
  `reference/Globals-*.md` files, matrix rows.
- [ ] Verify: spot-check that major constant families (cmXXXX, sfXXXX, ofXXXX,
  evXXXX, kbXXXX) each have a classification row; DOS/memory procs are NOT-PORTED with reasons.
- [ ] Commit: `git commit -m "docs(audit): batch K — globals & constant families"`

---

## Task 12: Part 2 behavioral-capability sweep → `concept-coverage.md`

**Files:**
- Modify: `docs/audit/concept-coverage.md`

**Interfaces:**
- Consumes: nothing from Tasks 1–11 (independent source: Part 2, pp. 93–314).
- Produces: the capability checklist + the receiving-end of all `→ concept`
  flags emitted by Tasks 1–11.

- [ ] **Step 1:** Dispatch a strong-model auditor with this brief: "Read the
  Part 2 chapters (pp. 93–314, in ≤20-page slices) — Views, Event-driven
  programming, Application objects, Window/dialog, Control objects, Data
  validation, Palettes/color, Editor/text, Collections, Streams, Resources. For
  each *behavioral capability* (not a single symbol) — event phase/routing,
  positional/focused/broadcast/masking, Z-order, modal group execution, cache
  buffers & lock/unlock draws, idle time, validate-on-focus-change/on-demand,
  grow modes, inter-view messaging, drag limits, help contexts, etc. — emit a row:
  capability · guide pages · present-in-port? (cite `src/` evidence) · covered in
  mdBook? (`docs/book/src/`, cite chapter or 'GAP')." Return the markdown table.
- [ ] **Step 2:** Integrate into `concept-coverage.md`. Then append every
  `→ concept` route collected from the Task 1–11 reference files (grep them).

```bash
grep -rn "→ concept" docs/audit/reference/ >> /tmp/concept-routes.txt
```

- [ ] **Step 3:** Verify the checklist covers each Part 2 chapter (one+ row per
  chapter) and each `→ concept` route is represented.
- [ ] **Step 4:** Commit: `git commit -m "docs(audit): Part 2 behavioral-capability sweep"`

---

## Task 13: Roll-ups — coverage matrix totals + the three reports

**Files:**
- Modify: `docs/audit/coverage-matrix.md`, `docs/audit/gap-report.md`,
  `docs/audit/rustdoc-scorecard.md`, `docs/audit/concept-coverage.md`

- [ ] **Step 1:** Tally per-section counts from `reference/*.md` into
  `coverage-matrix.md` (#ported / #equivalent / #not-ported / #missing / #suspect,
  avg doc score), with a grand-total row.

```bash
grep -rohE "\| (PORTED|EQUIVALENT|NOT-PORTED|MISSING) \|" docs/audit/reference/ | sort | uniq -c
grep -roh "SUSPECT" docs/audit/reference/ | wc -l
```

- [ ] **Step 2:** Build `gap-report.md` in three sections: **(1) Missing** — copy
  every `MISSING` row; **(2) Wrong** — copy every `SUSPECT` row with its cited
  divergence + page; **(3) NOT-PORTED register** — every `NOT-PORTED` row + reason
  (the permanent "do not re-flag" list).
- [ ] **Step 3:** Build `rustdoc-scorecard.md`: every public `PORTED`/`EQUIVALENT`
  symbol with doc score < 3, grouped by section, listing what's missing (what vs
  how). Add a per-section average.
- [ ] **Step 4:** Cross-link all four reports from `coverage-matrix.md` (top of file).
- [ ] **Step 5:** Verify the matrix grand totals equal the raw `grep` counts from
  Step 1 (no rows lost in roll-up).
- [ ] **Step 6:** Commit: `git commit -m "docs(audit): roll up coverage matrix + gap/doc reports"`

---

## Task 14: Self-verification — TOC reconciliation + anti-hallucination spot check

**Files:**
- Modify: `docs/audit/coverage-matrix.md` (append a "verification" section)

- [ ] **Step 1: TOC reconciliation.** Dispatch a fresh reviewer: "Here is the
  Part 3 TOC entry list (sections + page numbers, pp. 317–586) and here are the
  files in `docs/audit/reference/`. List every TOC section with **no**
  corresponding reference file or matrix row." Resolve every miss (dispatch a
  catch-up auditor for it) — do not proceed with gaps.
- [ ] **Step 2: Anti-hallucination spot check.** Dispatch a fresh reviewer over a
  random sample (≥1 section per batch, plus all of TView/TProgram/TEditor): "For
  each cited `tv::` symbol, confirm it exists in the named `src/` file (grep).
  For each `SUSPECT`, confirm the divergence is real and is **not** a documented
  D-rule deviation. For each `MISSING`, confirm no counterpart exists. Report any
  false rows."
- [ ] **Step 3:** Fix any false rows the spot check found (re-dispatch the owning
  auditor; never hand-edit a verdict without re-reading source).
- [ ] **Step 4:** Append a "Verification" section to `coverage-matrix.md`: the
  reconciliation result (0 missing files), the sample list, and the spot-check
  outcome.
- [ ] **Step 5:** Verify no section is unaccounted for and the spot check is clean.
- [ ] **Step 6:** Commit: `git commit -m "docs(audit): verify — TOC reconciliation + spot check"`

---

## Done when

- `docs/audit/coverage-matrix.md` reconciles 1:1 against the Part 3 TOC (Task 14
  reports 0 missing sections).
- Every `reference/*.md` row carries all three axes; no unresolved `UNSURE`.
- `gap-report.md` lists every **missing** and **wrong** finding (+ the NOT-PORTED
  register); `rustdoc-scorecard.md` lists every sub-bar public symbol;
  `concept-coverage.md` covers Part 2 + all `→ concept` routes.
- The anti-hallucination spot check is clean.
- All artifacts committed on `audit/tv2-guide-coverage`; **no `src/`, rustdoc, or
  mdBook file changed** (`git diff --stat main -- src docs/book` is empty).
