# tvision-rs — session handover (forward-looking)

> What the **next** session needs: current state, what's next, and the
> non-obvious gotchas. The per-session implementation narrative + the git-commit
> changelog live in
> [`docs/IMPLEMENTATION-LOG.md`](file:///home/oetiker/checkouts/tvision-rs/docs/IMPLEMENTATION-LOG.md).
> Read this, then [CLAUDE.md](file:///home/oetiker/checkouts/tvision-rs/CLAUDE.md)
> (orientation / locked decisions / cross-cutting seams), then start.
>
> **Direction:** the 92-class port is ✅ complete, the post-port backlog
> (`docs/BACKLOG.md`, Phases A/B/C) is exhausted, and the developer docs (Docs
> Phases 1–3) are ✅ done. The work now is **post-port UI polish + targeted
> tvision-rs-original extensions** (e.g. the `Splitter`; an interactive-resize pass; a
> dialog design guide + a `TabBar`/`PageStack` widget pair + a color-picker
> rebuild). When something lands: add an IMPLEMENTATION-LOG section and update
> this file.

## Current state (2026-06-20) — ✅ DATA-MOVEMENT EFFORT MERGED TO MAIN

**The data-movement effort (Phases 1–5) + tcv polish is now MERGED to `main`**
(merge commit integrating `consumer-api-coverage` alongside the parallel
TV2-audit documentation sweep). The `consumer-api-coverage` worktree has served
its purpose. The spec + plans live under `docs/superpowers/`; the per-commit
narrative is in `docs/IMPLEMENTATION-LOG.md`.

**What landed (all two-stage + final-reviewed clean; integrated-tree gate green:
1292 lib tests, `clippy --all-targets`, `fmt`, `xtask test`/`docs`):**
- **Axis A + C.1** (`45d5f04`, `f9d611e`) — the original `tcv` consumer-api gaps:
  window decoration flags made public + builders; `get_help_ctx` bubbles to the
  focused child (idle status-line path, faithful to `TView::TopView`).
- **Phase 1 — `exec_view_with<R>`** — by-value modal results via a generic
  `exec_view_capture<R>` core; migrated `color_dialog`/`theme_editor`; deleted the
  `ColorPick`/`ThemeEdit` `ModalCompletion` variants + both `Rc<Cell>`/`Rc<RefCell>`
  sinks. `pub fn exec_view_with<R>` at `src/app/program.rs:1288`.
- **Phase 2 — widen `FieldValue` + the `Custom` open seam** — added
  `Bool`/`Bits`/`List` + `Custom(Rc<dyn CustomValue>)` (the typed-at-the-edges
  escape for third-party components) with `custom`/`as_custom`/loud
  `value_as::<T>() -> Result<_, FieldTypeError>`; manual `PartialEq` (Custom =
  `Rc::ptr_eq`); cluster `value()`/`set_value()` as `Bits`;
  `Group`/`Window`/`Dialog` `gather_list`/`scatter_list` (whole record as ordered
  `FieldValue::List`). `Color`/`Theme` stay by-value, deliberately NOT `FieldValue`.
- **Phase 3 — sync signals → trait methods** (`22e054c`..`e2a472e`) — retired every
  cluster-B *sync* downcast from the pump. The four sibling-scrollbar read-syncs
  (Scroller / list viewers / Outline / Editor) collapsed into ONE defaulted
  `View::apply_scroll_sync(h, v, ctx)` + ONE `Deferred::ScrollSync { target, h, v }`
  (rename+generalization of `apply_list_scroll`); the pump calls it by virtual
  dispatch (the editor target rides the `#[delegate(to=editor)]` forwarder to the
  inner `Editor`). The two non-scroll syncs were de-downcast in place (variant kept,
  own hook, §2.1 reason): Indicator → `set_indicator_value`, PageStack →
  `apply_page_sync`. `ScrollBarSetParams` (write dir) + `SplitterDivider` deliberately
  left downcasting (out of the "five scroll-family brokers" scope). Behavior-preserving;
  no snapshot change. Final whole-branch review (Opus): Ready to merge = YES.
- **Phase 4 — modal-result reads via `FieldValue`** (`b1f5641`..`5a245cf`) — retired
  the multi-child downcasts from the `FindPick`/`ReplacePick` `ModalCompletion`
  arms. New defaulted `View::set_modal_data(FieldValue)` hook (distinct from
  `set_value`); `Editor` overrides it to parse the ordered `List` the pump assembled
  from the dialog's fields via `View::value()`. `field_bits` helper added. Dead
  `set_find_str`/`set_replace_str`/`set_editor_flags` setters + `CheckBoxes::as_any_mut`
  removed. **Recorded §2.1 deviation:** per-field `value()` reads instead of
  `gather_list` (inherent, not trait-reachable without a downcast) — within spec.
  **Recorded exceptions:** `ThemeColorPick` (payload is `Color`, not `FieldValue`)
  + dialog-OPEN pre-fill reads (structural "build UI from known widget state").
  Docs updated (`apps/dialogs.md`, `internals/custom-view.md`, IMPLEMENTATION-LOG).
  Gate: 1277 lib tests, clippy, fmt, examples green; no snapshot changes.
- **Phase 5 — generic ExecView from a view** (`efb1f2d`..`0cea466`) — closed
  consumer-API gap #2. New `Deferred::OpenModal { view, requester, then_command }`
  + `Context::request_exec_view(view, requester, then_command)` — the view-launched
  counterpart of `Program::exec_view_with<R>` (a view holds only `&mut Context`,
  never `&mut Program`). The pump stashes the boxed modal into `pending_modal` with
  the existing `RouteModalAnswer` completion; no new `ModalCompletion` variant, no
  downcast. `tcv`'s Info box is now a real custom `Dialog` built by `build_info_dialog`
  and launched via `request_exec_view`. Data-back `FieldValue` path deliberately
  deferred (result is the close command only; reason recorded on `request_exec_view`
  doc, §3.4/§2.1). Gate: 1279 lib tests, clippy, fmt, examples green.
  Also in this stack (tcv example polish): the mock catalog was enriched to 72
  entries for livelier incremental search; a user-directed scrollbar fix
  (`8c64e98`) wakes the `DirBox` v-bar (publish range+steps post-insert via a
  one-shot `ensure_inited`, mirroring `ListBox::new_list`); the Info dialog
  was widened so long descriptions don't clip the last field; and the catalog
  window now follows terminal resizes (`0cea466`) — grow-modes on the window +
  children (a deliberate deviation from TCV.PAS's fixed-size `GrowMode := $00`,
  recorded), with new `HeadlessHandle::resize` test-infra. All two-stage reviewed.
  **This completes the data-movement effort (Phases 1–5).**

**The driving design — spec v5 (read before Phase 3):**
`docs/superpowers/specs/2026-06-18-unified-data-movement-design.md` — unify the
port's fragmented data-movement into one typed currency + one mechanism per kind,
open to third-party components. Read **§3.5** (the `Custom` extensibility story:
runtime-checked / fail-loud / typed-at-the-edges; `value_as`; `TypeId`-version
caveat; the `inventory`-`self_check` follow-on rejected-for-now in §6.7) and **§2.1**
(the apply-with-judgment guard). Plans:
`docs/superpowers/plans/2026-06-18-data-movement-phase1-exec-view-with.md`,
`docs/superpowers/plans/2026-06-19-data-movement-phase2-fieldvalue-widen.md`.

**Phase 3 — DONE** (sync signals → trait methods, `22e054c`..`e2a472e`; see the
stacked-list entry above + `docs/IMPLEMENTATION-LOG.md`). The §2.1 judgment call the
spec deferred to Phase 3 was settled: only the three genuine sibling-scrollbar reads
(+ the existing list-viewer sync) fold into `apply_scroll_sync`; Indicator/PageStack
keep their own hooks (their payloads aren't a scrollbar `(h,v)` delta — reasons
recorded on the variant docs). Plan:
`docs/superpowers/plans/2026-06-19-data-movement-phase3-sync-trait-methods.md`.

**Phase 4 DONE.** See the "stacked on the branch" entry above and `docs/IMPLEMENTATION-LOG.md`.

**Phase 5 DONE.** See the "stacked on the branch" entry above and `docs/IMPLEMENTATION-LOG.md`.
The **data-movement stack (Phases 1–5) is now feature-complete on the branch.**

**Deferred follow-ons (recorded, not dropped):** `inventory`-collected
`Program::self_check()` + per-component `data_self_check` (needs a dependency
decision — spec §3.5/§6.7); `MultiCheckBoxes::value()` → `Bits` for coherence;
the ExecView data-back `FieldValue` path (see `request_exec_view` doc, §3.4/§2.1);
the ship-as-is Minors in the SDD ledger.

**Disposition (user):** MERGED to `main` on 2026-06-20 per explicit user call
("merge with main"). The whole stack landed together, integrated with the
parallel TV2-audit documentation sweep that had advanced `main` independently.

**Method note:** this work is run **subagent-driven** (CLAUDE.md "How to run the
port" / `superpowers:subagent-driven-development`): fresh implementer per task →
two-stage review (spec then quality, fresh agents) → fix → integrate, with a broad
final whole-phase review. The `sdd/progress.md` ledger in the worktree is the
recovery map (commits + per-task review verdicts + deferred Minors).

## Current state (2026-06-17, going-public prep)

The crate/repo/brand rename to **`tvision-rs`** is on `main` (the old name was
taken on crates.io), and a README "how it was built / where it's going" section
landed. Crate-level rustdoc + README now point at the repo `docs/` Markdown for
LLM-friendly docs. **Still pending for launch:** publish to crates.io
(`cargo publish -p tvision-rs-macros` then `-p tvision-rs`) and verify the
Pages site at the new URL.

New example **`examples/tcv.rs`** (`f47eaa3`) — a faithful re-port of the
author's 1993 Turbo Pascal program *Tobi's Catalog Vision* (search-as-you-type
catalog browser, embedded mock data). Building it as an outside *consumer*
surfaced three framework gaps the internal widgets never hit. **Next actionable
work:** [`docs/specs/2026-06-17-consumer-api-gaps.md`](file:///home/oetiker/checkouts/tvision-rs/docs/specs/2026-06-17-consumer-api-gaps.md)
— (1) window decoration flags are `pub(crate)`; (2) no generic deferred
`ExecView` for custom modals from a view; (3) `get_help_ctx` doesn't bubble to
the focused child (C++ `TGroup::getHelpCtx`). Suggested order 1 → 3 → 2; the
example documents each workaround in its header and should be made faithful as
each lands.

## Current state (2026-06-15)

**Branch `dialog-design-guide` HEAD = `5ab651a` (NOT yet on `main`); 1258 lib
tests green; clippy `--all-targets` + fmt clean; examples build.** The porting +
docs phases are behind us; work now proceeds on small feature/UX branches off
`main` (branch-first, fast-forward merge, two-stage review per task).

Most recently landed (tvision-rs-original UI; **full narrative in IMPLEMENTATION-LOG**):
the `Splitter` widget + interactive-resize pass (`Deferred::SplitterDivider`
broker + target-cycling `KeyboardResizeCapture`), then the **dialog layout guide +
`TabBar` + `PageStack` + color-picker rebuild** (below).

### Landed on `dialog-design-guide` (ready to merge to `main`)
The **dialog design guide + `TabBar` + `PageStack` + color-picker rebuild** is
**complete** — 13 commits (`9dbb9ed`..`5ab651a`), all two-stage reviewed, gate
green. Spec
`docs/superpowers/specs/2026-06-15-dialog-design-guide-and-tabbar-design.md`,
plan `docs/superpowers/plans/2026-06-15-dialog-design-guide-and-tabbar.md`. Four
pieces:
1. **Guide + constants + helper** — `docs/design/dialog-layout.md`; layout
   constants in `src/dialog/layout.rs` (`STD_BUTTON` 10×2, `BUTTON_GAP` 2, margins
   3/2/2, `BUTTON_ROW_FROM_BOTTOM` 3) + `ButtonRowAlign`; `Dialog::button_row`.
2. **`TabBar`** (`src/widgets/tab_bar.rs`) — cluster-shaped single-row selector
   (the `TMonoSelector : public TCluster` precedent): `selected`/`find_sel`/`press`,
   press-on-release, `value`/`set_value` transfer, corner-cap active tab `┌Label┐`,
   `~X~` hotkeys + ←/→ wrap, broadcasts `Command::TAB_BAR_CHANGED` (source = self).
3. **`PageStack`** (`src/widgets/page_stack.rs`) — a content multiplexer
   (`#[delegate(to=group)]`, N page Views, one `sfVisible`). **New cross-cutting
   seam — the third D3 sibling-broker** (after scroller↔scrollbar,
   listviewer↔scrollbar): `Deferred::PageStackSync { page_stack, tab_bar }` +
   `Context::request_sync_page_stack` + a pump arm beside `SyncScrollerDelta` that
   reads `tab_bar.value()` → `PageStack::set_active(idx, ctx)`. Bind the bar via
   `PageStack::bind_tab_bar(id)`; insert both as siblings in a `Group` and tab
   clicks/arrows auto-switch the page through the pump.
4. **Color-picker rebuild** (`src/dialog/colorpick/`) — `ColorPicker` is now a
   `Group` = `TabBar` + `PageStack` (four surfaces wrapped as `SurfacePage<S>` page
   Views over one shared `Rc<RefCell<ColorModel>>`) + an always-visible
   `InfoColumn`. The bespoke `drag.rs` broker is **retired** (only the
   `ColorDragRegion` enum kept); draggable surfaces self-drive via
   `ctx.start_mouse_track`. Gray chrome (tab row + info column off the blue
   `FramePassive`/`ScrollerNormal` roles onto gray `StaticText`/Label; surfaces
   keep their own colorful content). Tabs renamed `Hue/Sat`/`Xterm` (hotkeys
   P/R/H/X). Public API preserved (`new`/`color`/`select_tab`/`as_any_mut` + `Tab`).
   `tvdemo`/`gallery` re-laid via `button_row` (`5ab651a`).

**Merge note:** branch off `main`; the only non-obvious bundled change is a
re-captured `docs/book/src/screens/splitter.html` (pre-existing capture drift, not
from this effort — see IMPLEMENTATION-LOG). **`docs/demo/tvdemo.webp` is NOT
regenerated** (the movie covers the picker scene but `cargo xtask demo` has no
per-scene targeting — a full regen is a separate task).

## Previous state (2026-06-13, Docs Phase 3 landed + crate renamed to `tvision-rs`)

**The crate is now `tvision-rs`** (was `tvision`); the proc-macro crate is `tvision-rs-macros`;
the `tv::` house-style alias is unchanged (`tv = { package = "tvision-rs" }`). See the
rename entry at the top of IMPLEMENTATION-LOG. Upstream `magiblot/tvision` and the
C++ source paths (`source/tvision`, `scratch/tvision-spec`) are NOT renamed.

**Code HEAD ≈ `9490dbe` (rename) + this doc pass; 1183 lib + 15 xtask + 7 doctests
green; clippy `--all-targets` + fmt clean; `cargo xtask test` + `cargo xtask docs`
clean (link check + 0 leftover include directives).**

Docs Phase 3 made the guide self-verifying (full IMPLEMENTATION-LOG section at
top). The keystone is a **`cargo xtask test`** gate: it runs `rustdoc --test`
per chapter with `--extern tvision-rs=<rlib> -L deps` (NOT mdBook's `book.test`,
which can only pass `-L` and so can never resolve `use tvision_rs::…`). Guide code
blocks were triaged — user-facing snippets converted to compiling doctests via a
hidden `# use tvision_rs as tv;` + uncalled `# fn _demo(recv: &mut tv::Foo){…}`
wrapper; genuinely-internal sketches kept `rust,ignore` with an explicit
`// Illustrative sketch …` label. `docs.yml` now runs `cargo build --examples`,
`cargo test --doc -p tvision-rs`, and `cargo xtask test` before `cargo xtask docs`.

**Phase-3 gotchas for the next editor:**
- **Doctest convention:** in the book, the crate is `tvision-rs`, NOT `tv`. Any new
  ```` ```rust ```` guide block must add a hidden `# use tvision_rs as tv;` (or
  `extern`-free `# use tvision_rs::…;`). For method calls on a live `Program`/
  `Context`/view, wrap in a hidden uncalled `# fn _demo(recv: &mut tv::Foo){…}`.
  After editing, run `cargo xtask test` — it prints the real rustc error.
- **Never silence unused-var warnings with visible `let _ = …` in teaching code**
  — use a hidden `# #[allow(unused_variables)]` on the wrapper fn.
- **Example-backed `{{#rustdoc_include}}` blocks stay `rust,ignore`** (their
  example compiles via `cargo build --examples`; the per-block include is not a
  standalone program). Do not try to doctest them.
- **107 pre-existing rustdoc warnings** (broken intra-doc links like
  `ov_handle_event`, `ov_set_state`, `View::as_any_mut`) surface during `cargo
  xtask docs`. They are NOT Phase-3 regressions and the build does not fail on
  them — a separate "complete src rustdoc to parity" pass (below) would clear them.

### Two docs phases landed earlier this session (pre-Phase-3 snapshot `e8ce8ee`):

- **Docs Phase 1 — guide-page Rust-first pass** (`f36162e`). The 16 narrative
  pages under `getting-started/`, `apps/`, `internals/` were rewritten so the
  primary prose reads for a Rust dev with zero Turbo Vision knowledge; C++ demoted
  to skippable parentheticals or a uniform `> **Turbo Vision heritage:**`
  blockquote. `port/` (veteran chapter) + `reference/symbol-map`/`deviations` stay
  C++-aware **by design** — do not "fix" their C++ mentions.
- **Docs Phase 2 — widget gallery** (`340842a`, `ed08caf`, `17e95e9`, `a6ce0a1`,
  then `c6dbc4d`/`9cb916d`/`2161782` for determinism+polish). A parameterized
  `examples/gallery.rs` renders one widget per run (`cargo run --example gallery
  -- <name>`; no-arg lists names); each widget is a `// ANCHOR: <name>` builder
  the guide includes verbatim. **21 widgets.** New first-class guide page
  `docs/book/src/gallery.md` (SUMMARY "Widget Gallery" section) pairs each
  screenshot with its anchored code; key captures embedded in controls/dialogs/
  menus. `xtask Screen` gained an `args` field; capture uses `-N` (full-width
  bars); a committed `examples/gallery_fixture/` makes the file/dir dialogs
  deterministic; new `ColorPicker::select_tab(Tab)` lib method.

**The Phase-3-relevant artifact:** the gallery's `// ANCHOR:` builders are *real
compiling code* (they build in the example binary) but they are **`fn`-fragments,
not whole programs** — see the Phase 3 gotcha in "Next".

### `tv::Splitter` — RESOLVED (fully landed; see "Current state" above)
The `tv::Splitter` that was in-flight here is now complete on `main` (layout
solver, `joined()` line-joining, interactive resize, demo). This stale in-flight
note is retained only so older references resolve.

## Previous state (2026-06-12, docs content + user-facing cleanup landed)

**Code HEAD = `2e2153b`; 1177 lib tests + 15 xtask tests green; clippy
`--all-targets` + fmt clean; `cargo xtask docs` OK + link check clean.**

This session authored **all the developer-docs content** and ran a **user-facing
quality pass** over both doc layers (on top of the Plan 1 tooling machine, merge
`2b3656a`):
- **All 32 guide pages authored** (Parts I–V) — Part I vertical slice, then
  Parts II–V via a subagent author→review pipeline. Fixed the screenshot-clobber
  bug (`looks_blank()` guard in `xtask/src/screens.rs` — a blank tmux capture
  used to overwrite the committed HTML).
- **User-facing cleanup (rustdoc + guide):** stripped porting bookkeeping (row
  numbers, `Dn` labels, FOUNDATION/MECHANICAL, internal-doc refs, breadcrumbs);
  audited every stale "deferred" against the code (the real Deferred-channel
  *feature* kept); **rewrote rustdoc primary prose Rust-first** — a Rust dev who
  never used Turbo Vision can read it — with all C++ confined to a concise
  `# Turbo Vision heritage` section per item; added **Guide cross-links** from
  all 22 modules into their narrative chapter.
- **Project renamed to `tvision-rs`** (branding only: crate stays `tvision-rs`, namespace
  stays `tv::`, C++ origin stays "Turbo Vision").
- **Guide IA:** `port/faithful.md` = philosophy+gateway; `reference/deviations.md`
  = the canonical "Differences from C++ Turbo Vision" (`#d1`..`#d15` anchors,
  visible `D2 ·` numbers); new first-class `port/capture.md`; `modal.md` narrowed.
- Spec + plan:
  `docs/superpowers/specs/2026-06-12-api-docs-user-facing-cleanup-design.md` +
  `docs/superpowers/plans/2026-06-12-docs-user-facing-cleanup.md`.

Key commits (newest first): `2e2153b` guide cross-links · `183d3f4` rustdoc
Rust-first · `6cf9383` D-numbers visible · `d56c9b3` cleanup+rename · `2a28bb5`
guide Parts II–V · `35b3aca` Part I + screenshot guard.

**Tooling recap (Plan 1, `2b3656a`):** `cargo xtask docs` builds the mdBook guide
*and* rustdoc into ONE Pages site (guide at root, rustdoc at `/api/`, Guide⇄API
toggle, owned book↔api link checker, rustdoc into isolated `<target>/tvision-rs-rustdoc`).
Also `--serve` and `cargo xtask screens`. **Repo-owner action (still pending):**
Settings → Pages → Source = "GitHub Actions" before `.github/workflows/docs.yml`
can publish. Mermaid runtime JS is still a placeholder.

### Porting state (unchanged since `5407109`, configurable keymap)

Since the last handover two non-PORT-ORDER changes landed (both 2026-06-12, see
IMPLEMENTATION-LOG): the **default theme pinned to canonical RGB**
(`Color::BIOS_RGB`/`bios_rgb`), and a **configurable global keymap**
(`src/keymap.rs`: WordStar default / CUA / Emacs presets, shared by `editor` +
`input_line`, switchable live via tvedit's `Options ▸ Keyboard mapping`). The
keymap fixed the editor's plain-Backspace no-op bug. **Possible follow-up:**
extend the keymap to the remaining `ctrl_to_arrow` widgets (`cluster`,
`list_viewer`, `scrollbar`, `history`, `outline`) for cross-widget uniformity
under non-default presets.

Phase A + Phase B + Phase C are fully complete (all rows ✅).
Post-backlog latent edge notes resolved this session:
- **`fexpand`**: `std::path::absolute` replaces `canonicalize` in `FileEditor::new`.
- **`efBackupFiles`**: backup-rename (`foo.txt~`) implemented in `FileEditor::save_file`.
- **`edReadError`**: deferred-error seam (`pending_load_error`) shows read failure
  via `request_message_box` on first `handle_event`.
- **`initHistory`/`doneHistory`**: stale TODOs retired from `application.rs` (moot —
  thread-local Vec auto-inits/drops; row 54 deviation documented in `history.rs`).

Remaining latent edge notes (not worth fixing now):
- `input_line.rs:334` auto-fill clamp — blocked on auto-fill validator
- `editor.rs:952` setBufSize shrink — memory-only, no correctness impact
- `editor.rs:2238` charScan.scanCode — already correct behaviour
- `editor.rs:963` OOM path — Rust structural limitation (Vec infallible)
- `menu_session.rs:1159` TMenuPopup Ctrl+letter — dead under capture-stack model

### Phase C progress
- **C1 ✅ (`b388492`)** — editor find/replace dialogs + `do_search_replace`. The
  view-triggered async-modal seam (`Deferred::OpenFindDialog`/`OpenReplaceDialog`
  → `pending_modal` → `ModalCompletion::FindPick`/`ReplacePick`), built like
  `OpenSaveAsDialog`. Dialogs ported verbatim from `tvedit2.cpp` (with `THistory`
  arrows + `CheckBoxes` options). The `efPromptOnReplace` prompt is routed through
  the existing `request_message_box` seam (`answer_to = self` + `then_command =
  cmSearchAgain`); the answer is stored in `Editor::pending_replace_answer` (via a
  `set_modal_answer` override) and consumed on the `cmSearchAgain` re-run.
  Two-stage reviewed. See IMPLEMENTATION-LOG.
- **C2 ✅ (`2ee829c`)** — editor right-click context menu. The `MouseDown`
  right-button arm now builds the 4-item `Menu` (Cut/Copy/Paste/Undo with
  `kbShiftDel`/`kbCtrlIns`/`kbShiftIns`/`kbCtrlU`) and calls `popup_menu()` (the
  row-52 implementation). Global position = `m.position + self.abs_origin`. No new
  seam — `popup_menu` queues the deferred effects inline. Two-stage reviewed.
- **C3 ✅** — internal-clipboard editor (`insertFrom` branch + clipboard
  `EditWindow`). `Editor::is_clipboard` field + `insert_from` method +
  `selection_bytes` helper. `clip_copy`/`clip_paste` dual-path (internal vs OS).
  `update_commands` faithful guard (skip CUT/COPY/PASTE for clipboard editor; PASTE
  gated on `clipboard_has_selection` snapshot). Three new `Deferred` variants +
  `Context` snapshot fields + three pump drain arms. `EditWindow::handle_event`
  hides instead of closing when hosting the clipboard editor. Caller API:
  `ctx.register_clipboard_editor(editor_id, window_id)`. Two-stage reviewed.
- **C4 ✅ (`5f57bb7`)** — D10 gather/scatter group-walk. `Group::gather_data()` /
  `scatter_data()` walk `children.iter()` (forward = C++ `last→prev`). New
  `View::set_value_ctx` seam (default: delegates to `set_value`) lets `ListBox`
  override to republish its v-bar via `focus_item_num`. Macro forwarder added.
  Clears the `list_box.rs` deferral TODO. Two-stage reviewed.
- **C5 ✅ (`7923ac9`)** — cmQuit veto + saveAs modified-close inline drives.
  `valid_end` now drives `OpenMessageBox` (and `OpenSaveAsDialog`) inline — the
  quit prompt fires instead of silently re-spinning. `validate_modal_close`
  extended to handle `OpenSaveAsDialog` inline: FileDialog runs, `pump_once`
  services the re-injected `cmSave`, then re-validates so the close goes through.
  `drive_save_as_inline` helper de-duplicates the two sites. `LIMITATION`
  breadcrumb removed from `FileEditor::save`. Two-stage reviewed.
- **C6 ✅ (`386eb84`)** — cmDosShell terminal suspend/resume seam.
  `Backend::suspend()`/`resume()` trait methods (no-op defaults; CrosstermBackend:
  `suspend` = `restore_terminal()`, `resume` = re-enter alt-screen/raw/mouse).
  `Renderer::invalidate_all()` clears front buffer → full repaint after resume.
  `program_handle_event` threaded with `renderer`; `Command::DOS_SHELL` arm:
  suspend → writeShellMsg → `raise(SIGTSTP, cfg(all(unix,not(test))))` → resume →
  invalidate_all. `libc` dep added. Two-stage reviewed.
- **C7 ✅ (`8c9bf85` + `20871fd`)** — help-ctx refresh / OneOf status line.
  `View::get_help_ctx()` trait method (default: delegates to
  `ViewState::get_help_ctx()`); forwarder in `tvision-rs-macros/src/specs.rs`;
  `delegate_view` spy updated (27 methods). `Program::pump_once` idle arm wires
  `TStatusLine::update()`: reads `captures.top_modal_view()` as tvision-rs's
  `TheTopView` equivalent, calls `v.get_help_ctx()`, then `sl.set_help_ctx(top_ctx)`
  (which is now idempotent — early-return guard mirrors C++'s `if(helpCtx!=h)`).
  OneOf status defs switch automatically when a modal dialog with a matching
  `helpCtx` is the top view. Two-stage reviewed.
- **C8 ✅ (`f38c8d3`)** — theme editor + D7 extension point. `Theme::set_style` /
  `Role::name` / `pub(crate) const ALL/ROLE_COUNT` (minimal D7 runtime-mutation
  API). `Deferred::OpenColorDialogForRole { editor_id, role, fg, current }` +
  `Context::open_color_dialog_for_role` — same async-modal seam as C1. Two new
  `ModalCompletion` variants: `ThemeColorPick` (per-role color picker result routes
  back to `ThemeEditorBody` via downcast) and `ThemeEdit` (reads working theme on
  OK, writes to `Rc<RefCell<Option<Theme>>>` sink). `Program::set_theme` (install +
  `invalidate_all`). `Program::theme_editor()` — 64×24 modal with `ThemeEditorBody`
  inner widget + Fg/Bg/OK/Cancel buttons. `ThemeEditorBody` (new
  `src/dialog/theme_editor.rs`): scrollable list of all 75 roles with fg/bg swatches
  + "AaBb" preview; arrow/PgUp/PgDn/Home/End navigation; F/B hotkeys +
  `cmThemeEditFg`/`cmThemeEditBg` commands. Two-stage reviewed.
- **C9 ✅ (`95e0f47`)** — bracketed-paste. `Event::Paste(String)` variant (removes
  `Copy` from `Event`; 3 full-copy sites get `.clone()`; all existing `match *ev`
  arms bind Copy fields and compile unchanged). `EnableBracketedPaste` wired at
  setup/resume; `DisableBracketedPaste` at restore. `crossterm::event::Event::Paste`
  translated to `Event::Paste`. `HeadlessHandle::push_paste`. Editor `Event::Paste`
  arm: `mem::take` + `ev.clear()` + `insert_text`. Two-stage reviewed.

**Phase C is now complete (all C1–C9 ✅).** Walk BACKLOG.md for any remaining rows.

### What is on `main` from the Phase A/B backlog run (committed):
- **B1 ✅ (`680aabc`)** — button `cmCommandSetChanged` graying; `Program::new` seeds `command_set_changed: true` for initial broadcast. InputLine `can_update_commands`/`update_commands` from `handle_event` + `set_state`.
- **B3 ✅ (`680aabc`)** — InputLine cmCut/cmCopy/cmPaste; `Deferred::InputLinePaste` broker; `paste_text` (save_state + max_len clamp + check_valid).
- **B6 ✅ (`6ae0222`)** — FileDialog/ChDirDialog `wfGrow`; screen-relative resize deferred to first `handle_event`; `SearchRec` attr/size/time from `std::fs` + `pack_dos_time`.
- **B5 ✅ (`c917b4b`)** — `View::on_bounds_changed` hook; `Scroller::on_bounds_changed`; `list_viewer::on_bounds_changed` free fn (resize formula) adopted by all 5 ListViewer concrete types; Outline uses Scroller formula; `Window::locate` re-pushes `set_zoomed`; `KeyboardResizeCapture` (full keyboard resize: arrows/Ctrl/Home/End/PgUp/PgDn/Enter/Esc); `cmResize` enabled when `sfSelected && (wfMove || wfGrow)`. Resolves TODO(33d).
- **B8 ✅ (`dae38c1`)** — `InputLine::set_value` clamps to `max_len` (UTF-8-safe, mirrors paste_text); `valid` calls `ctx.request_focus(id)` before returning false (faithful to C++ `select()`). Timer-payload was pre-resolved. init/doneHistory + help-ctx propagation remain standing deferrals.

## Previous state (2026-06-10, end of the backlog-run session)

**HEAD = `5757565`; 1104 lib tests green; clippy + fmt clean (run both
default and `--no-default-features`); `cargo build --example hello` works.**

The 92-class port is complete (PORT-ORDER all ✅; rows 81–87 dropped in favor
of the truecolor color-picker extension, rows 91–92 terminal family done).
This session ran the **backlog run** end to end:

- **Phase A COMPLETE — all FOUNDATION seams**, two-stage reviewed each:
  - **A1 🔴 CommandSet denylist flip** (`faabc78`) — faithful `initCommands`
    semantics (everything enabled, 5-command seed); the allowlist + file-dialog
    bandaid deleted; **`Context::command_enabled(cmd)`** per-pump snapshot
    query. `docs/design/command-enablement.md`.
  - **A2 🔴 resetCurrent cascade** (`6a58919`) — currency is a tree property:
    `Group::currency_dirty` at insert → post-order `settle_currency` (eager in
    `Program::new`, pump step 2b); `set_visible_descendant` (hide direction);
    remove parity (tgroup.cpp:112). **Keystone:** `set_current` clears the
    dirty flag FIRST, incl. its early-return leg — protects explicit focus.
    Fixed the latent nested keyboard-dead-window gap.
  - **A3 MouseAuto + MouseTrackCapture** (`f07d4e0`) — the pump synthesizes
    `Event::MouseAuto` (440 ms delay / 110 ms cadence, tevent.cpp+hardwrvr.cpp
    derivation); `MouseTrackCapture` is a pure router (`Deferred::MouseTrack`,
    loop bodies stay in widgets). Recipe: `docs/design/mouse-track.md`.
  - **A4 theme chain verification** (`66e7527`) — every `theme.rs` value
    derived from the literal C++ palette chain and documented inline; cyan
    window scheme (`FrameCyan*`); `ListRoles` + `ListViewer::list_roles()`
    (the `THistoryViewer::getPalette` virtual successor).
  - **A5+B4 phased key dispatch** (`43c9d30`) — `Phase` rides `Context` (the
    `owner_size` pattern); button/label/cluster plain-hotkey accelerators +
    `ctrl_to_arrow` landed with it. A focused view consuming a letter starves
    the post-process loop (faithful — that's why dialogs use Alt).
  - **A6 OS clipboard (user directive)** (`dfba123`) — the faithful
    `TClipboard` chain in `src/backend/clipboard.rs`: arboard native → OSC 52
    emit → internal mirror (last resort only); `os-clipboard` default-on
    feature; NO OSC 52 read; `HeadlessHandle::clipboard()/set_clipboard()`
    test accessors. `docs/design/os-clipboard.md`. **Bracketed paste is
    deliberately deferred to C9** — do not enable `EnableBracketedPaste`
    before consuming `Event::Paste`, or terminal-paste silently dies.
  - **B7 RAII terminal lifecycle (user directive)** (`7827235`) —
    `CrosstermBackend::new()/with_color_depth` are fallible and own raw
    mode/alt screen/mouse capture; Drop + panic hook + unix signal thread
    (`128+signum`) restore; at-most-one-live-instance contract documented;
    `hello.rs` main is 3 lines.
- **B2 COMPLETE — all 8 press-and-hold adoptions** on the A3 seam (the
  `while(mouseEvent(...))` loops from the TODO audit): button, scrollbar
  (arrow auto-repeat + thumb drag), inputline (edge scroll + drag-select),
  cluster (press moved to release-over-same-item — the C++-correct
  semantics), frame close icon (release-confirm), listviewer + outline
  (skip-counters 4/3, `dragged<2` graph-toggle gate), statusline
  (drag-highlight via the drawSelect matrix, post-on-release), editor
  (drag-select with persisted `selectMode`, edge auto-scroll, in-hold wheel
  forwarding to the bars, middle-button pan; bonus fix — untracked wheel no
  longer positions the cursor, faithful to TEditor's eventMask).
- **Pump-semantics change — know before touching `pump_once`** (`eb7648d`):
  the deferred drain is **hoisted out of the `!ev.is_nothing()` dispatch
  gate** — it runs for every picked `Some(ev)`, consumed-by-pre-route or not
  (pre-route deferreds are first-class; the old LATENT COUPLING silent drop
  is gone). `sync_gate_bounds` runs at the **top of the dispatch gate**
  (covers same-pump resize relayout + all previous drains). Four old
  "drain is gated on !ev.is_nothing()" comments were corrected — don't
  reintroduce the assumption.

## PAUSED in-flight work

*(none — all paused worktrees integrated this session)*

## Next

**Active:** none — the **dialog guide + `TabBar` + `PageStack` + color-picker
rebuild** is complete on `dialog-design-guide` (see "Landed" above). Next step is
**merge to `main`** (fast-forward) once reviewed; then optionally regenerate
`docs/demo/tvdemo.webp` (full movie regen) and migrate `msgbox`/`inputbox`/
`filedlg`/theme-editor onto the new `layout.rs` constants + `button_row` (they
already conform behaviorally — a de-dup pass, recorded out-of-scope in the spec).

**Splitter resize follow-ups (noted in the resize spec, not started):** Cyan/Gray
window-palette divider color (the splitter uses the Blue frame-role family — other
palettes need palette threading into the splitter); larger Ctrl+arrow *divider*
nudges (the window target already does ±8/±4).

The docs phases are done and self-verifying. Older optional candidates, none
committed:

- **Clear the 107 rustdoc warnings** — broken intra-doc links (`ov_handle_event`,
  `ov_set_state`, `View::as_any_mut`, …) surfaced by `cargo xtask docs`. These
  predate Phase 3 and don't fail the build, but fixing them would let `docs.yml`
  add `RUSTDOCFLAGS=-D warnings` as a stricter gate. Pairs with "complete
  `src/theme/` rustdoc to parity" below.
- **Convert remaining illustrative sketches** only if you make the relevant
  internals public — today they reference `pub(crate)`/pump-local items, so they
  are correctly labeled `rust,ignore` rather than force-compiled.
- A new **feature phase** would need its own planning (the porting backlog is
  exhausted — Phase A+B+C all ✅).

When editing the guide, follow the Phase-3 doctest convention in "Current state"
(hidden `# use tvision_rs as tv;` + `# fn _demo(recv){…}` wrapper; run
`cargo xtask test`).

### Verifying docs edits
- Integrated tree, `CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target`.
- `cargo xtask test` (guide doctests — prints the real rustc error), `cargo test
  --doc -p tvision-rs` (src doctests), `cargo build --examples`, then `cargo xtask
  docs` (regenerates screenshots ≈40s deterministic; builds + link-checks).
- After any `{{#rustdoc_include}}` edit, grep the built HTML for leftover
  directives (`grep -rl rustdoc_include docs/book/book` must be empty) — the link
  checker won't catch an unresolved include. Include-path depth: `gallery.md` is
  in `src/` → `../../../examples/…`; pages one level deeper → `../../../../`.

**Smaller follow-ups:** complete `src/theme/` rustdoc to parity;
`#![doc(html_logo_url/favicon)]` crate attrs; vendor the real mermaid runtime;
optionally **hyperlink** the heritage `(deviation Dn)` citations (currently plain
text — would need per-item relative rustdoc→guide paths, link-checked).

**Intentionally left:** C++ references inside `#[cfg(test)]` doc comments (test
provenance, **not** rendered in rustdoc) — a separate large pass only if wanted.

**Porting backlog:** Phase A + B + C fully ✅ — exhausted. A new *feature* phase
would need its own planning, separate from the docs work above.

**Standing principle (this session):** the port is DONE, so docs/comments never
say "deferred" — only "ported" or "deliberately not ported, with a reason" (the
`no-deferred-state` memory). The rustdoc primary prose assumes **zero C++
knowledge**; C++ lives only in `# Turbo Vision heritage` sections.

**C1 reuse note for later rows:** the find/replace prompt reused the
`request_message_box` async-modal seam (`answer_to` + `then_command`) and the
`Deferred::OpenXxxDialog` → `pending_modal` → `ModalCompletion::XxxPick` pattern.
C2/C9 dialogs should follow the same shape rather than inventing new seams.

## Editor seam leftovers (still open, latent)

- **`edReadError` on load** (ctor has no ctx) — breadcrumbed.
- **`FileEditor::saveAs`** is DONE (`Deferred::OpenSaveAsDialog` →
  `ModalCompletion::SaveAsPick`; accept test is `!= CANCEL` — FD_OK_BUTTON
  ends with `cmFileOpen`, not `cmOK`). The `widgets::editor_mut` hatch peels
  FileEditor/Memo to the inner `Editor` for the brokers.
- **cmQuit veto + saveAs modified-close** — **RESOLVED in C5** (`7923ac9`).
  `valid_end` now drives the prompt inline; `validate_modal_close` drives
  `OpenSaveAsDialog` inline via `drive_save_as_inline`.

## Non-obvious gotchas (read before starting)

- **Worktrees** live under `/scratch/oetiker/claude-worktrees/<project>-<name>`.
  Create manually (`git worktree add <path> -b <branch>`) and dispatch
  non-isolated subagents pointed at the path. **Give each parallel agent its
  own `CARGO_TARGET_DIR`** (e.g. `/home/oetiker/scratch/cargo-target-<tag>`)
  — a shared target dir makes their "clean" claims unreliable. ALWAYS
  re-verify on the integrated tree with the canonical
  `CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target`.
- **Run `git merge` in `/home/oetiker/checkouts/tvision-rs`**, never inside a
  worktree — `cd <worktree> && git merge <branch>` merges the branch into
  itself ("Already up to date") and the gates then run on the wrong tree.
  (Bit this session repeatedly.)
- **Commit completed rows before dispatching worktree subagents that build
  on them** (a worktree branches from the last commit).
- **Shared 128-core machine, max 4 cores for compile/test:** `-j 2` +
  `--test-threads=2` per agent, at most two building agents in parallel.
- Verification is **snapshot tests** (D11, `insta`) for anything that draws.
  `cargo-insta` is **not installed** — generate via `INSTA_UPDATE=always`,
  hand-verify, commit.

## Standing deferrals (still open — grep the TODOs)

- **idle→`statusLine->update()` help-ctx refresh** — inert under a single
  `All` `StatusDef`; worth doing only when a context-split `OneOf` line
  lands (needs `View::get_help_ctx` + a TopView resolver).
- **`program_handle_event` modal-isolation** breadcrumb; the
  `ModalFrame`/`DragCapture` "(0,0)-desktop absolute-coords" caveat (the bar
  shifts the desktop down by 1).
- **`max_len` clamp on `InputLine::set_value`** (row-39 gap; → B8).
- **RESOLVED this session** (so stale memories don't resurrect them):
  CommandSet allowlist (A1), resetCurrent cascade (A2), the theme
  "provisional values" problem (A4 — trust the documented chains), the
  status-line drag-highlight, ALL `TODO(row 31, D9)` hold loops, and the
  editor mouse/wheel deferrals (B2).

## Standing process reminders

- **Subagent-driven** (CLAUDE.md "How to run the port"): per row → fresh
  implementer (Sonnet for MECHANICAL, strongest model for FOUNDATION) →
  **two-stage review** (fresh SPEC then QUALITY agents — never self-review
  in the main thread) → fix (implementer for substantive findings,
  orchestrator for one-liners) → integrate → commit. Briefs are
  **self-contained** (inline the C++ + D-rules + existing types).
- **FOUNDATION rows: read-only design investigation first** (a Plan agent
  maps the constraint surface; the orchestrator decides the design; the
  implementer gets the approved spec verbatim). This caught real gaps in
  A2/A3/A5/A6 before any code existed.
- **`git diff` the whole tree** after an implementer before integrating —
  out-of-scope changes are a real failure mode (a B2 implementer modified
  the pump unprompted; review caught it and the proper redesign landed).
- When you add a `View` trait method, add a matching forwarder to
  `tvision-rs-macros/src/specs.rs` (the `delegate_view` spy test catches
  existing methods, not brand-new defaulted ones). A new `Deferred` variant
  needs NO forwarder. Validator-trait methods are NOT `View` methods.
