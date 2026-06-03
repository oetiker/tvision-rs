# Session handover — Row 27 `TScroller` DONE. Next (per PORT-ORDER): Row 28 `TListViewer` → list substrate → Phase 4

> Living handover for the **next** rstv session. Read this, then
> [CLAUDE.md](file:///home/oetiker/checkouts/rstv/CLAUDE.md) (orientation /
> Current state / Next step), then start. When the next stage lands, update or
> replace this file for the session after.
>
> **Direction = [`docs/PORT-ORDER.md`](file:///home/oetiker/checkouts/rstv/docs/PORT-ORDER.md).**
> It is dependency-ordered; follow it in sequence rather than treating "tracks" as
> an open choice. Lowest-numbered incomplete rows are the work. The
> "Parallelizable batches" section (e.g. Batch C validators 58–62) lists fan-outs
> that *may* run concurrently — an efficiency, not a competing direction.

## Where things stand (git `main`)

| commit | what |
|--------|------|
| `43e5c68` | **Validator wave — TValidator (35) + TInputLine (39) + D10 `value`/`set_value`** → Batch B COMPLETE |
| `8ea87cb` | test: end-to-end modal `valid()`-veto (exec_view → Dialog → Group → InputLine) |
| `44c404e` | docs: HANDOVER — sync commit table + 440 tests + the valid()-veto test |
| `543b2c8` | **TScroller (27)** — cross-view scrollbar broker (FOUNDATION) ← THIS session |

**Build state:** 457 lib + 3 integration + 2 doctests green; `cargo clippy
--all-targets -- -D warnings` and `cargo fmt --check` clean. Working tree clean.
(Cargo artifacts land in `/home/oetiker/scratch/cargo-target` — set
`CARGO_TARGET_DIR`.)

**Phase 2 COMPLETE. Batch B (Phase-3 leaves) COMPLETE.** Phase-1 rows 27/28/32 +
Phase-4 (46+) remain. **Row 27 `TScroller` DONE** this session (FOUNDATION; the
cross-view broker pattern — see below). Next incomplete in PORT-ORDER sequence:
**row 28 `TListViewer`** (FOUNDATION), then 48 `TListBox` / 32 `TApplication`,
then Phase 4 (menus/status).

## What landed THIS session — Row 27 `TScroller` (`543b2c8`, FOUNDATION)
`TScroller` (base for `TEditor` 66, `TTextDevice`/`TTerminal`, `TOutlineViewer`)
holds two sibling scrollbars and mirrors their `value` into its own `delta`. Built
main-thread/Opus: brief → Opus implementer → two-stage review (SPEC then QUALITY,
fresh C++-adversarial agents). Brief: `docs/briefs/row27-tscroller.md`.

**THE pattern this row establishes — the cross-view scrollbar broker (reused by
rows 28 + 66):** a leaf view holds only `&mut Context` during dispatch (D3) and so
can neither **read** nor **mutate** its window-frame sibling scrollbars. So the
**pump is the cross-view broker in both directions** — it owns the tree and
performs every sibling read/write at deferred-apply time (`group.find_mut(id)` +
`as_any_mut`/`View::value()`), mirroring the existing `ChangeBounds`/`FocusById`
apply arms.
- **`Event::Broadcast` left UNTOUCHED** (the advisor-blessed call): `{command,
  source}` stays as is; `source` is only the **filter** (scroller reacts iff
  `source ∈ {h_id, v_id}`). The faithful successor to C++ "read `value` off the
  `infoPtr` subject" is "the pump resolves the subject and reads its `value`" —
  **not** stuffing the value into the message. (First real consumer of
  `Broadcast{source}`.)
- **Read** (`scrollDraw`): `Deferred::SyncScrollerDelta{scroller,h,v}` → pump reads
  each bar's `value` via `View::value() → FieldValue::Int`, then downcasts the
  scroller (`as_any_mut`) and calls `apply_delta` (the `setCursor(cursor+delta-d)`
  adjust + `delta = d`, guarded by `d != delta`).
- **Write** (`setLimit`/`scrollTo`): one flexible `Deferred::ScrollBarSetParams{id,
  value,min,max,page_step,arrow_step}` with **per-field `Option` = "preserve the
  bar's LIVE field where `None`"** (pump fills `None` from the live scrollbar, then
  `set_params`). Serves rows 28/66 too (`tlstview.cpp`'s `setRange`/`setStep`).
- **Visibility** (`showSBar`): `Deferred::SetVisible(id,bool)` (no `StateFlag::
  Visible` — D8 dropped it; the painter honors `ViewState.state.visible`).
- **Dropped (D8):** `drawLock`/`drawFlag`/`checkDraw`/all `drawView()` — deferred
  mutation + whole-tree redraw make the synchronous re-entrancy guard structural.
- **New seams:** `FieldValue::Int(i32)` (first consumer); `ScrollBar::value()` →
  `Int` + `ScrollBar`/`Scroller` `as_any_mut` overrides; `Role::ScrollerNormal`
  (provisional, `TODO(window-scheme remap)`).
- **Deferred + breadcrumbed:** `changeBounds` does **not** re-publish bar params on
  resize (`TODO(resize)`) — faithful-safe because no consumer resizes a scroller
  until `TEditor` (66), which must call `set_limit(ctx)` after its resize.
  `Role::ScrollerSelected` (cpScroller idx 2) deferred to row 66 (the base
  `TScroller` inherits `TView::draw` — a **uniform** `getColor(1)` fill, no
  selected branch; a spec-review catch).

## What landed the PRIOR session (validator wave, `43e5c68`)
The full row-35→39 wave + the **D10 typed-value protocol**, built as one Opus
implementer + full two-stage review (SPEC then QUALITY, fresh C++-adversarial
agents). Brief: `docs/briefs/row35-39-validator-inputline.md`.

- **TValidator (35)** → `src/validate.rs`: object-safe abstract `Validator` trait
  (D2) — `is_valid_input(&self,&mut String,bool)` / `is_valid(&self,&str)` /
  `error` / `is_status_ok` (all defaults accept) + provided non-virtual
  `validate`. **`transfer` deliberately omitted** (PORT-ORDER row 35 lists it, but
  it has no overrider until TRangeValidator row 59 → would be a dead stub; the
  row-34 "no dead stubs" rule wins). `tv::Validator`.
- **D10 value protocol** → `src/data.rs`: **`FieldValue`** typed-transfer currency
  — one `Text(String)` variant, **grows per control** (Role/Glyphs convention;
  `Bits(u32)` for cluster + `Int` for range land when those wire their value).
  Defaulted **`View::value(&self)->Option<FieldValue>` / `set_value(&mut self,
  FieldValue)`** (the getData/setData successors). The dialog **gather/scatter
  group-walk is DEFERRED** to its first consumer (inputBox / Batch E) —
  breadcrumbed in `data.rs`.
- **TInputLine (39)** → `src/widgets/input_line.rs`: faithful `tinputli.cpp` port.
  Draw (scrolled `moveStr` + ◄/► arrows + selection redraw + cursor), full
  keyboard (nav / word-nav / edit / Ins-toggle / Shift-block-extend /
  printable-insert with the `maxLen && maxWidth && maxChars` guard / Ctrl-Y),
  single-shot mouse positioning **+ the faithful single edge-click scroll-by-one**,
  validator `save_state`/`restore_state`/`check_valid`, `valid(cmd)` (faithful
  return), `set_state`→`select_all`, `value`/`set_value`.
  **Key correction the implementer caught:** `first_pos` is a display **COLUMN**,
  not a byte offset (the brief mis-stated it; `cur_pos`/`sel_*`/`anchor` ARE byte
  offsets). All `data` indexing steps through grapheme helpers — **D13
  panic-safe** (multi-byte tests over `ä€中` BITE).
- **New seams:** `text::prev` (`TText::prev`), `DrawCtx::put_str_part` (`moveStr`'s
  `begin` column-skip), 3 theme roles `Input{Normal,Selected,Arrow}` (provisional
  gray, `TODO(row 34 gray theming)`) + 2 glyphs (◄ U+25C4 / ► U+25BA), `cmValid`,
  `State::cursor_ins`.
- **End-to-end veto test (`8ea87cb`, advisor-flagged):** the headline
  `InputLine::valid()` behavior — a modal must NOT close on OK while a child's
  validator rejects — lived only in isolated widget tests. The actual veto is in
  `exec_view`'s outer `while !valid(end_state)` loop. New integration test in
  `program.rs`: a `Dialog` + `InputLine` + `RejectAll` validator, driven through
  `exec_view` with pre-queued `[cmOK, cmCancel]`, asserts the result is **cmCancel**
  (cmOK vetoed, modal stayed open) + the `ModalFrame` popped. Bite-verified; **no
  bug in the veto path** (`exec_view` honors `valid()` correctly). The `[OK,
  CANCEL]` shape is deliberate — `[OK]` alone loops forever (a permanently-rejecting
  field can never close, which IS faithful). + a `#[cfg(test)] Dialog::insert_child`
  hook.

### Deferred + breadcrumbed in the validator wave (prior session; grep the TODOs)
- **clipboard** cmCut/cmCopy/cmPaste — no `Context` clipboard seam (backend has
  set/get_clipboard; not surfaced to views). `TODO(clipboard)` in `input_line.rs`.
- **command-graying** `updateCommands`/`canUpdateCommands` (enable/disable cmCut/
  Copy/Paste) — needs the `Context` command-set query that **TButton also
  deferred**. `TODO(button/inputline: command-set query …)`. **Menus (Phase 4)
  force this** — add a read-only command-set accessor to `Context` then.
- **mouse press-and-hold / drag-select loops** — `TODO(row 31, D9)`; single-shot
  positioning + the single edge-click scroll only.
- **`valid()`'s `select()` focus side-effect** — C++ focuses the invalid field
  before returning false; needs `&mut Context` + the **focus-by-ViewId** seam
  (`Deferred::FocusById` / `request_focus`, already built at row 41).
  `TODO(valid-select)`. The **return value is faithful** (gates modal OK).
- **validator `transfer` hook** — `TODO(row 59)` at both `value`/`set_value`
  sites; TRangeValidator will produce a typed non-`Text` value (→ `Int`).
- **`Validator::error`→msgbox** — `TODO(msgbox row 63)`.
- **`cur_pos` re-clamp hazard** — `TODO(row 59/62)`: a future *mutating* validator
  that SHRINKS `data` could leave `cur_pos` past EOS / mid-grapheme → D13 panic.
  Unreachable now (abstract validator never mutates); re-clamp when the first
  auto-fill validator (Range/PXPicture) lands.

## NEXT — follow PORT-ORDER in sequence

Lowest-numbered incomplete rows = the work. Next up:

### Row 28 `TListViewer` (FOUNDATION) — the immediate next row
Base for all list widgets (`TListBox` 48, history, color/file lists). Like
`TScroller` it drives 2 scrollbars — **reuse the row-27 cross-view broker verbatim**
(`Deferred::SyncScrollerDelta`/`ScrollBarSetParams`/`SetVisible`, `View::value()`).
C++ `source/tvision/tlstview.cpp` (already previewed in the row-27 work: `setRange`
→ `ScrollBarSetParams`, `focusItem` → `setValue`, `setState` → show/hide, broadcast
read → sync). New for 28: the **list-render matrix** (focused/selected/normal/
disabled cells, D7 roles — `ListNormal`/`ListSelected*` already seeded in theme.rs),
multi-column layout (`numCols`), `focusItem`/`selectItem`, hot mouse/keyboard nav,
and the abstract `getText`/`isSelected` hooks subclasses fill. FOUNDATION → Opus +
two-stage review.

### Then, in PORT-ORDER order
- **`TListBox` (48, MECHANICAL)** — concrete `TListViewer` over a collection
  (`Vec`); typed value (D10) — first `value`/`set_value` consumer beyond TInputLine;
  may pull in the **dialog gather/scatter group-walk** (still deferred, see below).
- **`TApplication` (32, MECHANICAL)** — thin tile/cascade/dosShell wrapper over
  `TProgram`; independent, slot in anytime.
- **Phase 4 — menus + status line** (the path to a fully drivable app):

  **Menus:** `TMenuItem`/`TSubMenu`/`TMenu` (46, FOUNDATION — the menu data tree;
  C++ `operator+` builders → a Rust builder API) → `TMenuView` (49, FOUNDATION —
  hotkey/shortcut dispatch, the `evBroadcast` mask) → `TMenuBar` (50) / `TMenuBox`
  (51) / `TMenuPopup` (52, popup exec via D9). **Menus force the deferred
  `Context` command-set query** (command graying) — build that read-only accessor
  on `Context` when you hit it (additive; the deferred-effects refactor stabilized
  `Context::new` for *effects*, a read accessor is a separate additive concern).
- **Status line:** `TStatusItem`/`TStatusDef` (47) → `TStatusLine` (53,
  FOUNDATION — hint()/help-ctx→hint mapping).
- Wiring menus + status line into `Program` lets the `examples/hello.rs` demo grow
  a real menu bar + status line (and shifts the desktop down — revisit the
  `ModalFrame`/`DragCapture` "(0,0)-desktop absolute-coords" caveats then).

### Available parallel fan-out (efficiency, not a competing direction) — Batch C: concrete validators (58–62, MECHANICAL)
Fully unblocked by `TValidator` (35); **fully parallel among themselves** → the
clean worktree fan-out cadence (Sonnet implementers, `isolation:"worktree"`,
orchestrator integrates + pre-seeds any shared files). These are PORT-ORDER's
"Parallelizable batches" — run them concurrently whenever convenient; they don't
displace the in-sequence FOUNDATION work above. C++ all in `tvalidat.cpp`:
- **58 `TFilterValidator`** (char allow-list), **59 `TRangeValidator`** (int range;
  **resolves the deferred `transfer` hook + the `cur_pos` re-clamp hazard** above —
  and now has `FieldValue::Int` ready [added by row 27]; so this one is
  FOUNDATION-ish, do it carefully),
  **60 `TLookupValidator`** (abstract lookup), **61 `TStringLookupValidator`**,
  **62 `TPXPictureValidator`** (Paradox picture-mask state machine — the big one;
  `picture()`/`process()`/`scan()`/`group()`/`iteration()` — sets `status=vsSyntax`,
  which is what `is_status_ok()` and TInputLine `valid(cmValid)` already consult).
Each validator's `is_valid_input` may **mutate** `s` (auto-fill) — that's the
trigger for the TInputLine `cur_pos` re-clamp `TODO(row 59/62)`.

### Then `msgbox` (63) + Batch E fan out
`messageBox`/`inputBox` (`msgbox.cpp`) is buildable now (TButton + TStaticText +
TInputLine exist) but is the **first consumer of the D9 view-triggered async-modal
path** (`Deferred::OpenModal` + posted completion `Command`) — guide D9 "exec_view
— corrected" carries that design; build when a menu/msgbox needs it (Phase 4), not
before. Batch E dialog families (color/file/chdir/editor/outline/textview) fan out
once their leaf prereqs exist.

## Standing process reminders
- **Fan-out cadence is for gap-free MECHANICAL leaves only** (parallel worktree
  implementers, `isolation:"worktree"`, Sonnet, orchestrator integrates shared
  `mod.rs`/`lib.rs` + pre-seeds `theme.rs`). **FOUNDATION rows → per-row, Opus,
  full two-stage review.** Commit completed rows before dispatching worktree
  agents that build on them (worktree branches from the last *commit*).
- **Two-stage review stays mandatory** (SPEC then QUALITY, fresh C++-adversarial
  agents against the **C++ + guide, NOT the brief** — the brief can be wrong, as
  the validator wave's `first_pos` mis-statement proved). Make round-trip/unit tests
  **discriminating + bite-checked** (verify a finding fails before/passes after).
  Both stages keep earning their keep: at row 27, **spec** review caught an invented
  active/selected `draw` branch (the base inherits `TView::draw`'s uniform fill) and
  **quality** caught `std::any`-vs-`core::any` + a stale doc; in the validator wave,
  quality caught the untested validator reject/restore path and spec caught a dropped
  double-click scroll.
- **Snapshot workflow** (Appendix B step 4): `cargo-insta` is NOT installed →
  generate a `.snap` with `INSTA_UPDATE=always cargo test <name>`, verify by hand,
  re-run plain, commit the `.snap`.
- Keep per-row briefs **tight + self-contained + inline** (over-long briefs crashed
  a Sonnet implementer's context earlier in Batch B).

## Older standing deferrals (still open, grep the code)
- **`Context` command-set query** (command-graying) — TButton + TInputLine both
  wait on it; **Phase-4 menus force it**.
- **phase signal on `Context`** (plain-letter postProcess accelerator) — 3 waiting
  consumers: button, label, cluster (`is_plain_hotkey` exists but is ungated).
- **`Group::remove` release-after-remove ordering** — a removed selectable child
  never gets `RELEASED_FOCUS{source}`; a `TLabel` whose link is removed at runtime
  keeps a stale `light`. C++ `hide()`s before `removeView`. No consumer hits it yet.
- **`cmResize` keyboard sub-mode** (`window.rs`); **scrollbar auto-repeat +
  thumb-drag** + **cluster drag-cursor** (`TODO(row 31, D9)`); **close
  press-and-hold confirm** (`frame.rs`); **sibling tee-walk** (`framelin.cpp`);
  **shadow casting** (`group.rs`); **gray multi-scheme theming**
  (`TODO(row 34 gray theming)` — realign provisional `*` colours, incl. the 3 new
  Input roles); **row-9 glyphs** continue per-widget.
- **ctrlToArrow / accelerator TODOs** in cluster/scrollbar — shared key helpers
  EXIST (`b53c618`); retire opportunistically.
