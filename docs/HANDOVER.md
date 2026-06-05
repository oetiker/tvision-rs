# Session handover — **`THistoryWindow` (56) DONE** (Phase 4). Next: **`THistory` (57)** — the FOUNDATION view-triggered async-modal path (`Deferred::OpenModal` + result flowback, shared with msgbox 63), which ALSO must build the **ModalFrame deliver-outside-to-modal** seam row 56 deferred. Then **Batch C validators 58–62** / **msgbox 63**

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
| `ad41f05` | **`THistoryWindow` (56) — modal recall window hosting the viewer** — faithful `thistwin.cpp`. `HistoryWindow` is a `TWindow` subtype (D2 embed + `#[delegate(to = window)]`, like `Dialog`) assembling a frame + two `standard_scroll_bar`s (h `sbHorizontal\|sbHandleKeyboard`, v `sbVertical\|…`) + a `HistoryViewer` (55) over an extent grown `(-1,-1)`, with `get_selection` = the viewer's focused `get_text` (by id + `as_any_mut` downcast). `flags = wfClose` only (not movable). **Seam promoted (shared foundation touch, also unblocks msgbox 63 + Batch E):** `Window::insert_child`/`Dialog::insert_child` go `#[cfg(test)]`→real `pub(crate)`, + new `pub(crate) Window::child_mut`. **Viewer `setup()` (the Context-needing ctor tail, row 55/ListBox constraint) runs ONCE at the TOP of `handle_event`, BEFORE delegating to `TWindow::handleEvent`** — so the first event reaches an initialized viewer (the bite: misorder → first Down hits a range=0 viewer → focused wrong; verified empirically to fail with focused==1). **DEFERRED (breadcrumbed, NOT a silent drop):** the C++ `evMouseDown && !mouseInView → endModal(cmCancel)` outside-click cancel — our `ModalFrame` (program.rs) **Consumes outside positional events before they reach the modal view**, so the arm is unreachable; delivering outside clicks to the modal view is a **modal-loop change reserved for row 57 / msgbox 63** (`Deferred::OpenModal`). Esc/Enter/double-click confirm/cancel still work (the viewer, row 55). `cpHistoryWindow` palette → provisional Window/Frame + TODO(row 34). **Two-stage review (fresh SPEC + QUALITY Opus, both PASS on production code, no blockers):** the two converged test-quality SHOULD-FIX items fixed — the setup-guard test rewritten into a TRUE bite (new `#[cfg(test)] Window::select_child_for_test` makes the viewer current; deliver Down; assert focused 1→2; empirically confirmed to fail when misordered) + the negative h-bar-max test made non-vacuous (asserts the exact `-32` queued max AND that it drains through `ScrollBar::set_params` in a live `exec_view` pump without panic — the HANDOVER watch-item, now pinned end-to-end). 613→618 tests. MECHANICAL ← THIS session |
| `6ada1fd` | **`THistoryViewer` (55) — modal recall list over the store** — faithful `thstview.cpp`. A single-column `TListViewer` subclass (mirrors the `TListBox` template: `impl ListViewer` lv/lv_mut + override only `get_text`; `impl View` delegating draw/event/nav to the `list_viewer::` free fns) that reads the row-54 store **live by id**. `get_text(item)` → `history_str(id, item)`; `handle_event`: Enter/double-click → `ctx.end_modal(Command::OK)`, Esc/`cmCancel` → `end_modal(Command::CANCEL)`, **unconditional (no `sfModal` gate** — the viewer only ever lives in a modal `THistoryWindow`, faithful to the C++), else fall through to `list_viewer::handle_event`. `history_width()` = max `text::width` over the channel. The Context-needing ctor tail (`set_range(history_count)`/`focus_item(1)` when range>1/h-bar `setRange(0, historyWidth()-size.x+3)`) moved to a post-insert **`setup(&mut self, ctx)`** (same Context-free-ctor constraint as `ListBox::new`; **does NOT** publish step sizes — the C++ ctor doesn't either). `history_id: u8` throughout (C++ `ushort` truncates to the `uchar` store; explicit `u8` avoids silent aliasing). Palette reuses provisional `Role::List*` + `TODO(row 34): cpHistoryViewer remap` (no new Role variants). **Two-stage review (SPEC + QUALITY, fresh Opus): SPEC PASS no findings; QUALITY PASS + 1 SHOULD-FIX** (added a bite-checked test for the previously-untested h-bar `setRange` branch — exact `max=-12` from `historyWidth-size.x+3`) + 1 cost NIT comment. 601→613 tests (+12 incl. a snapshot proving item-1 focus). MECHANICAL ← THIS session |
| `121ec67` | **history store (54) — `historyAdd`/`Count`/`Str`/`clearHistory`** — faithful `histlist.cpp` as an idiomatic `Vec`: a single **global 1024-byte budget** with **global FIFO eviction** (NOT per-id), `thread_local! { RefCell<Vec<HistRec>> }` (single-threaded like the C++; per-test isolation falls out of libtest's per-test threads). Four free fns in `src/widgets/history.rs`. Cost/entry = `str.len()+3` (UTF-8 bytes, faithful `TStringView::size`). Index order **oldest→newest per id** (row 55's `get_text` reads it directly — no inversion anywhere, faithful to C++). `history_add`: dedup `(id,str)` FIRST → evict front → push back. **Documented deliberate deviation:** the C++ front-sentinel + always-skip-front bookkeeping is NOT replicated, so every non-evicted entry is readable (C++ hides one globally-oldest entry after the first overflow); C++'s in-band sentinel makes its budget 3 bytes tighter (a byte-boundary nuance, not a new divergence). `initHistory`/`doneHistory` moot (thread-local Vec) — omitted, not stubbed. **Two-stage review (SPEC + QUALITY, fresh Opus): both PASS, no blockers; +3 NITs** (`#[must_use]` getters, single `cost_of()`, doc precision note). 593→601 tests. The shared dependency for rows 55–57. MECHANICAL ← THIS session |
| `0fc6a9e` | **`Desktop::tile`/`cascade` geometry + `cmTile`/`cmCascade` WIRED — the row-32 `TApplication` breadcrumb is CLOSED** — faithful port of `TDeskTop::tile`/`cascade` (`tdesktop.cpp`): `i_sqr`/`most_equal_divisors`/`divider_loc`/`calc_tile_rect` ported as pure fns (the C++ file statics threaded as params, no globals; `divider_loc` multiply in `i64`), re-added the `tile_columns_first` field (`favorY = !tile_columns_first`; tile now consumes it). **New seam `view::locate` is a FREE FN, NOT a `View` trait method** — a trait method would be forwarded by `#[delegate]` to a wrapper's inner group, whose `size_limits` is 0×0, bypassing e.g. `Window`'s 16×6 min (the advisor-caught trap; the existing inherent `Window::locate` for zoom is left untouched). `tile`/`cascade` are defaulted no-op `View` trait methods **overridden by `Desktop`** (mirrors `select_window_num`; the program drives the desktop by id through `&mut dyn View`, no downcast) + `Group::tileable_ids` (forEach order = `children.iter().rev()` filtered `tileable && visible`) + `child_mut` per child. **Off-by-one pinned** (`tile_num`/`cascade_num` start `n-1`; cascade error check subtracts the full `n`; `lastView` = `ids.last()`). Wired in `program_handle_event` after `group.handle_event`, beside the QUIT catch (`getTileRect()` = desktop child extent, `ev.clear()` after). `examples/hello.rs` opts its 3 demo windows into `ofTileable` + adds Window→Tile/Cascade items (cmTile/cmCascade are default-enabled, so they route + draw enabled). **Full two-stage review (SPEC + QUALITY, fresh C++-adversarial Opus): no blockers, no should-fix** — SPEC verified line-by-line incl. the end-to-end menu-enable path; QUALITY traced the integer geometry panic-free (`i_sqr(1)=1`, no div-by-zero) + tests discriminating. +3 NIT cleanups (closed a latent **`delegate_view` spy gap**: it never exercised `set_menu_current`, count 24→**25**; + column-first `most_equal_divisors` branch test; + cmCascade pump test). 585→593 tests (FOUNDATION) ← THIS session |
| `e02a4bf` | **Menu bar + status line WIRED INTO `Program` — the drivable-app payoff** — `examples/hello.rs` is now a real running TV app (menu bar row 0, desktop, status line bottom row). `Program` captures the menu-bar/status-line `ViewId`s + **seeds initial command-graying** at construction via `update_menu_commands` (the carried startup-regray gap: `cmCommandSetChanged` does not fire at startup). `pump_once` adds the faithful **`getEvent` status-line pre-routing** (`tprogram.cpp:153`): `evKeyDown` always + over-the-line `evMouseDown` (gated by new **`Group::topmost_child_at`** = `firstThat(viewHasMouse)`), run **BEFORE `captures.dispatch`** so accelerators (F10→cmMenu, Alt-X→cmQuit) fire even while a modal is open (the discriminating placement crux + bite). `StatusLine` keyDown **global-accelerator arm** (`tstatusl.cpp:181`): match keycode over ALL items incl. textless, **transform `ev`→`Command` in place, no clear** (propagates; NOT `ctx.post`+clear). **`MenuBar::update_menu_commands` override closed a latent gap** (graying was silently inert on the real bar — the existing broker test used a test-double). `Desktop::insert_view` → `pub` (production window-insert seam). idle→`update()` help-ctx refresh **deferred (inert under a single `All` `StatusDef`)**. Two-stage review (SPEC faithful, QUALITY no prod blockers; 2 vacuous mouse tests reworked into bite-checked discriminating ones). 576→585 tests (FOUNDATION) ← THIS session |
| `df3b8b9` | **Status line (rows 47 + 53) — `TStatusItem`/`TStatusDef` data + `TStatusLine` draw/data slice** — `src/status/` (`mod.rs` data + builder, `status_line.rs` view). The standalone snapshot-testable view (NOT yet wired into `Program`, mirroring how the menu draw layer landed before the modal/Program wiring). `HelpCtxRange::{All, OneOf(Vec<HelpCtx>)}` replaces C++'s numeric `[min,max]` help-ctx ranges (D1 corollary — string `HelpCtx` has no ordering); `StatusItem.text: Option<String>` (`None` = the hidden global-hotkey item: draws nothing, no width, but the keyDown loop matches it); command-graying via a cached `CommandSet` on the **view** (the `update_menu_commands` broker hook + `cmCommandSetChanged`→`request_update_menu`, NOT a field on `StatusItem` — faithful to C++ computing `commandEnabled` live). 6 `Status*` theme `Role`s. 551→576 tests (FOUNDATION) ← THIS session |
| `add2947` | **Menu MODAL layer Step-2 stage 3 (52) — `TMenuPopup`** — the LAST modal piece: `put_click_event_on_exit` flag on `MenuSession` (gates the bottom-level exit-click re-post; bar/box `true`, popup `false`), popup level starts `current=None` + clears its menu clone's `default` (`menu->deflt=0`), `popup_menu()` free fn + `auto_place_popup` geometry (faithful `popupMenu`/`autoPlacePopup`); `end_session_with` reworked to a kind-keyed (`is_bar`) teardown (a popup's level 0 IS a box). `TMenuPopup::handleEvent` moot/dropped (Ctrl+letter TODO). 545→551 tests (FOUNDATION) ← THIS session |
| `93d6d35` | **Menu MODAL layer Step-2 stage 2 (50–52)** — the **mouse** arms of the flattened `execute()`: `track_mouse`/`mouse_in_view`/`mouse_in_owner`/`mouse_in_menus`, `evMouseDown`/`Up`/`Move` step arms + per-level loop-locals (`last_target_item`/`mouse_active`/`first_event`); stage-1 `handle_key` refactored into one shared `run()` loop (kbd+mouse+cmMenu); `evMouseDown` bar activation (`do_a_select`) + `activate_mouse`; cmMenu routed through `run()` (FOUNDATION) |
| `ed0abfa` | **Menu MODAL layer Step-2 stage 1 (50–52)** — `MenuSession` capture handler = flattened `execute()`; keyboard nav + submenu recursion + the `putEvent`→parent re-apply loop; new `Deferred::OpenMenuBox`/`SetMenuCurrent` + `ctx.put_event` + `Group::insert_with_id` + `View::set_menu_current` (FOUNDATION) |
| `0687530` | **TMenuBar/TMenuBox DRAW/DATA layer (50/51)** — `MenuView` trait + `current` + draw/getItemRect + 6 menu theme roles (FOUNDATION) |
| `dfe66b1` | **TMenuView passive layer (49)** — command-graying broker + hotkey dispatch (FOUNDATION) |
| `c5c061d` | **TMenu data tree (46)** — `MenuItem`/`Menu`/`MenuBuilder` (FOUNDATION) |
| `fc66637` | **TListBox (48)** — first concrete `TListViewer` (MECHANICAL) |
| `3e6645f` | **TApplication (32)** — thin D2 wrapper over `Program` (MECHANICAL) |
| `47894f0…66ab55f` | **`#[delegate]` proc-macro** — `tvision-macros` crate + workspace, then **adopted** across cluster/Window/Dialog/ParamText/Label/Desktop + the hello example (replaces `cluster_wrapper!`) |

**Build state:** 618 lib (was 613; +5 this session: row-56 `HistoryWindow` tests
— construction, keyboard-routes-after-setup via `exec_view`, `get_selection`, the
setup-guard TRUE bite, the negative-h-bar-max live-pump end-to-end) + 5 integration
(3 `render_pipeline` + 2 `delegate_view`, the latter exercising **25** macro
forwarders) + 2 doctests green;
`cargo build --example hello` builds the drivable app; `cargo clippy --workspace --all-targets -- -D warnings` and `cargo
fmt --all --check` clean (verify clippy with a forced re-lint — a cached run can
mask a fresh warning). **It is a Cargo workspace**
(`tvision` + `tvision-macros`) — use `--workspace` for test/clippy/fmt. (Cargo
artifacts land in `/home/oetiker/scratch/cargo-target` — set `CARGO_TARGET_DIR`.)

**Phase 2 COMPLETE. Batch B (Phase-3 leaves) COMPLETE. Phase-1 row 32 COMPLETE.**
**Phase 4 in progress — Row 46 `TMenu` data tree + Row 49 `TMenuView` passive
layer + Rows 50/51 draw/data + the menu MODAL layer Step-2 stages 1 (keyboard), 2
(mouse) AND 3 (`TMenuPopup` 52) ALL DONE** (a prior session). **The
menu modal layer (rows 46/49/50/51/52) is COMPLETE** — the whole flattened
`TMenuView::execute()` (bar + box + popup, keyboard + mouse) is ported. **Status
line rows 47 (`TStatusItem`/`TStatusDef`) + 53 (`TStatusLine` draw/data slice) are
DONE** (a prior session). **The menu bar + status line are WIRED INTO `Program`**
(`examples/hello.rs` is a drivable TV app), and **`Desktop::tile`/`cascade` +
`cmTile`/`cmCascade` are WIRED** (the row-32 breadcrumb CLOSED) — all prior sessions.
The history store (54) + `THistoryViewer` (55) landed a prior session; **THIS
session: `THistoryWindow` (56) is DONE** (top git-table row), including the
promoted production `Window::insert_child`/`child_mut` seam. **Next: `THistory`
(57)** — the FOUNDATION view-triggered async-modal path (`Deferred::OpenModal` +
result flowback), which must ALSO build the **ModalFrame deliver-outside-to-modal**
seam row 56 deferred (the outside-click cancel). msgbox 63 is the co-consumer of
the async-modal seam. Batch C validators 58–62 remain an available parallel
fan-out; `cmDosShell` still needs a backend suspend seam.

> **Worktrees** live under `/scratch/oetiker/claude-worktrees/<project>-<name>`
> (global CLAUDE.md). A `WorktreeCreate` hook (`~/.claude/settings.json` →
> `~/.claude/worktree-create.sh`) redirects the Agent/Workflow
> `isolation:"worktree"` worktrees there, so **isolation IS usable** — BUT the
> hook only activates on a session **restart** (hooks load at startup); until
> then, isolation lands in the project's `.claude/worktrees/` and you should
> create the worktree manually at the `/scratch` path + dispatch a non-isolated
> subagent.

## What landed THIS session — history store (54) + `THistoryViewer` (55) (MECHANICAL)
The first two rows of the **history subsystem**, both Opus-orchestrated with the
standard cycle (advisor-vetted brief → Sonnet implementer → **two-stage review**,
fresh SPEC then QUALITY Opus agents → NIT fixes → integrate → commit). Briefs:
[`row54-history-store.md`](file:///home/oetiker/checkouts/rstv/docs/briefs/row54-history-store.md),
[`row55-history-viewer.md`](file:///home/oetiker/checkouts/rstv/docs/briefs/row55-history-viewer.md).
Both live in `src/widgets/history.rs` (store + viewer together). Detail is in the
two top git-table rows above; the load-bearing points:

- **Row 54 store** — one **global 1024-byte budget**, **global FIFO eviction**
  (NOT per-id — the trap that kills the obvious `HashMap<id, VecDeque>` port),
  `thread_local! RefCell<Vec<HistRec>>`, oldest→newest index order, dedup-before-
  evict. The advisor caught the C++ **front-sentinel + always-skip-front** byte-
  block artifact; we model the clean contract (every non-evicted entry readable)
  and **document the non-replication** rather than reproduce it.
- **Row 55 viewer** — a `TListViewer` subclass mirroring `ListBox`; the
  Context-needing ctor tail → a post-insert `setup()`; **unconditional** endModal
  (no `sfModal` gate — the viewer only lives in a modal window). The hbar
  `setRange(0, historyWidth()-size.x+3)` can go **negative** (small history, wide
  view) — faithful to C++, published as-is; now covered by an exact-`-12` test.

**The two seams rows 56/57 need (discovered this session — build these FIRST):**
1. **A production `Window::insert` (for row 56).** `Window::insert_child` /
   `Dialog::insert_child` are currently **`#[cfg(test)]`-only** — there is **no
   production path to add child controls to a window/dialog yet** (msgbox 63 and
   all Batch E dialogs were never built, so nothing needed it). `THistoryWindow`
   (56) is the **first** production consumer: its ctor inserts a `THistoryViewer`
   into the window group (after building two `standard_scroll_bar`s). So row 56
   must first promote `Window::insert_child` to a real `pub(crate)` production
   method (it's already ctx-free: `self.group.insert(view)`; same for `Dialog`).
   This is a tiny but genuine foundation touch that **also unblocks msgbox 63 +
   Batch E**. See `tdesktop.cpp`-style factory: `initViewer` grows the extent by
   `(-1,-1)`, builds the two `sbHorizontal|sbHandleKeyboard`/`sbVertical|…`
   bars, constructs the viewer, inserts it; then the window calls the viewer's
   `setup()` (needs a Context — so it lands post-insert, like `ListBox`).
2. **The view-triggered async-modal path (for row 57, shared with msgbox 63).**
   `THistory` (57, the dropdown icon next to a `TInputLine`) `execView`s a
   `THistoryWindow` **from within its own `handle_event`** and writes the picked
   string back into the linked input line. This is the **unbuilt D9 `OpenModal`**
   seam the menu sessions deliberately reserved (a *command* launching a modal,
   not menu nav) — `Deferred::OpenModal` + a posted completion `Command` + a way
   to read the modal's result (`THistoryWindow::getSelection` = the viewer's
   focused `get_text`, reached by id + `as_any_mut` downcast). **Design this with
   the advisor + main-thread care** — it is the FOUNDATION piece of the cluster,
   and msgbox 63 is its co-consumer (build the seam once, wire both).

## Prior session — menu bar + status line WIRED INTO `Program` (FOUNDATION)
The **drivable-app payoff**: the standalone menu-bar + status-line views become a
running app. Brief:
[`docs/briefs/row47-53-program-wiring.md`](file:///home/oetiker/checkouts/rstv/docs/briefs/row47-53-program-wiring.md)
(advisor-vetted — the advisor's key call was to **defer the idle→`update()` help-ctx
refresh**: under the single universal `All` `StatusDef` every real app uses,
`find_items` is invariant, so `update()` is observably inert — no consumer ⇒ no new
`View::get_help_ctx`/`TopView` seam this session) → Opus implementer → **full
two-stage review** (SPEC then QUALITY, fresh C++-adversarial Opus agents — SPEC
faithful with no blockers, QUALITY no production blockers) → 2 vacuous mouse tests
reworked into bite-checked discriminating ones → integrate. 576→585 tests.

- **`Program` ids + initial regray.** `Program::new` now captures the menu-bar +
  status-line `ViewId`s and **seeds command-graying directly** via
  `update_menu_commands(&command_set)` at construction — closing the carried gap that
  menus/status are born all-enabled and `cmCommandSetChanged` does **not** fire at
  startup. (No defer: the deferred queue is not drained on an idle first pump anyway.)
- **`getEvent` status-line pre-routing** (`tprogram.cpp:153`) in `pump_once`, at the
  **top of the `Some(ev)` arm, BEFORE `drop_disabled` + `captures.dispatch`** — because
  C++ `getEvent` pre-routes regardless of modal nesting, so accelerators fire **while a
  modal dialog is open** (the discriminating `accelerator_fires_during_a_modal` crux,
  bite-checked). keyDown always; `evMouseDown` only when the line is the topmost view
  under the cursor (new **`Group::topmost_child_at`** = faithful `firstThat(viewHasMouse)`
  over direct children). The pre-route does the `makeLocal` (`m.position -= origin`)
  the group router would normally do, since it bypasses the router.
- **`StatusLine` keyDown global-accelerator arm** (`tstatusl.cpp:181`, the last deferred
  arm): match the keycode over **ALL** items (incl. textless global hotkeys), and if
  `command_enabled`, **transform `ev`→`Event::Command` IN PLACE — no clear, no post** —
  so the same live event propagates into normal dispatch (porting it as `ctx.post`+clear
  would double-handle). The mouseDown arm (post+clear) was already there.
- **`MenuBar::update_menu_commands` override — closed a LATENT FOUNDATION GAP.** The bar
  never implemented this hook, so the command-graying broker fell through to the trait
  no-op: graying was **silently inert on the real `MenuBar`** for BOTH the new startup
  regray AND the pre-existing `cmCommandSetChanged` broadcast path (the row-49 broker
  test used a `#[cfg(test)] MenuProbe` test-double, so it never caught this). One-line
  delegate to the shared `menu_view::update_menu_commands`.
- **`examples/hello.rs` → a drivable app.** Faithful init insets (`initDeskTop`
  `r.a.y++`/`r.b.y--`, `initMenuBar` `r.b.y=r.a.y+1`, `initStatusLine` `r.a.y=r.b.y-1`);
  3 demo windows inserted into the desktop (via the now-`pub` `Desktop::insert_view`);
  a File/Window menu bar + the standard status line; `run()` spins the real
  `program.run()` loop. **Known limitation (documented):** menu items can only wire
  commands that already *route* — menu→dialog needs the unbuilt D9 `OpenModal` path
  (row 63), so **no About/Tile/Cascade items** yet. Alt-shortcuts reach the bar via
  `ofPreProcess` (the bar sets it + `Group::handle_event` runs the preProcess phase);
  F10 enters menus via the status-line accelerator → cmMenu.
- **Deferred + breadcrumbed (NOT stubbed):** idle→`statusLine->update()` help-ctx
  refresh (inert under a single `All` def — omit-until-consumer; revisit for a context-
  split `OneOf` line); `cmTile`/`cmCascade` + `Desktop::tile`/`cascade` geometry +
  `cmDosShell` (see NEXT); the status-line press-and-hold drag-highlight (`TODO(row 31,
  D9)`).
- **Verification:** 9 `wiring` tests — F10-enters-menu, Alt-X-quits, the
  accelerator-during-modal placement crux (bite: move pre-route after capture dispatch →
  red), two *reworked* discriminating mouse tests (status-line click past a modal gate;
  desktop click reaches a spy Probe and is NOT eaten by the line's clear — each
  bite-checked against its own production path), initial-regray (no pump), + a
  full-screen layout snapshot (bar row 0 / desktop / line row h-1).

## Prior session — status line (rows 47 + 53) (FOUNDATION)
The **draw/data slice** of the status line — a standalone, snapshot-testable
`TStatusLine` view (the `TProgram` getEvent/idle wiring is a separate next step,
mirroring how the menu draw layer landed before its modal/Program wiring). Brief:
[`docs/briefs/row47-53-status-line.md`](file:///home/oetiker/checkouts/rstv/docs/briefs/row47-53-status-line.md)
(advisor-vetted) → Opus implementer → **full two-stage review** (SPEC then QUALITY,
fresh C++-adversarial Opus agents — both PASS, no blockers) → 3 MINOR fixes →
integrate. New `src/status/` module (`mod.rs` + `status_line.rs` + 4 snapshots) +
`lib.rs`/`theme.rs` wiring. 551→576 tests.

- **`StatusItem` / `StatusDef` (row 47, `src/status/mod.rs`)** — pure data + a
  fluent builder (mirrors `MenuBuilder`). **`StatusItem.text: Option<String>`** is
  load-bearing: `None` (C++ `text == 0`) is a **hidden global-hotkey item** — it
  draws nothing AND consumes no horizontal space (the `i += l+2` advance is *inside*
  `if(text != 0)` in both `drawSelect` and `itemMouseIsIn`), but the (deferred)
  keyDown loop still matches it to fire its command. `key_code: Option<KeyEvent>`
  (`None` = `kbNoKey`).
- **`HelpCtxRange::{All, OneOf(Vec<HelpCtx>)}`** — **THE one real deviation** (a D1
  corollary). C++ `TStatusDef(min, max, items)` selects its items by a **numeric**
  help-context range `[min,max]`; our `HelpCtx` (D1) is a namespaced `&'static str`
  with **no ordering**, so contiguous integer ranges become an explicit membership
  set. `All` = the universal `[0,0xFFFF]` def every real app uses; `OneOf(set)` = the
  rare context-split (tvdemo `[0,50]`/`[50,0xffff]`). `find_items` = first-match walk;
  multi-def selection is faithful-but-unexercised this row (nothing sets a non-default
  help ctx yet) — supported in the data model + unit-tested via `set_help_ctx`.
- **Command graying via a cached `CommandSet` on the VIEW, NOT a field on
  `StatusItem`** (the advisor-flagged crux; the menu precedent misleads here).
  `TMenuItem` has a real `disabled` field the menu broker mutates; **`TStatusItem`
  has none** — C++ `drawSelect` calls `commandEnabled(T->command)` **live**. So the
  view caches one `Option<CommandSet>` snapshot (refreshed by the **same**
  `update_menu_commands` broker hook + the `cmCommandSetChanged`→`request_update_menu`
  broadcast arm, reused verbatim from the menu); `draw` tests `cmd_set.has(cmd)`,
  treating an unset cache as all-enabled (the same startup-regray gap menus carry).
  Status items are **flat** — the hook is non-recursive (unlike the menu's tree walk).
- **`draw` = `drawSelect(0)`** (faithful `tstatusl.cpp:62`): bg fill in `cNormal`,
  per-item leading/trailing space + `put_cstr`, the `i+l < size.x` clip, the 2×2
  color matrix (reuses the menu's `(enabled, selected)` shape via a `StatusColors`
  helper that mirrors `MenuColors` but reads the 6 new `Status*` roles), and the
  hint tail (`if i < size.x-2` → `│ ` separator (U+2502) + clipped hint via the
  `hint` closure). **Themes only, no palettes** — colors resolve from `Theme` via
  `Role` (`getPalette`/`getColor`/`cpStatusLine` are NOT ported; the C++ palette
  bytes only seeded the provisional theme colors, `TODO(row 34 gray theming)`).
- **`hint()` virtual → `Box<dyn Fn(HelpCtx) -> Option<String>>` closure** on the view
  (default `|_| None`; `with_hint`/`set_hint` setters) — the idiomatic port of the
  overridable C++ `virtual hint`.
- **`handle_event`:** the **mouse** arm (single-shot, faithful to the C++
  press-and-hold deferral): `item_mouse_is_in` hit-test (`mouse.y!=0→None`; `[i,k)`
  accumulation skipping textless items) → enabled-check → `ctx.post(cmd)` →
  unconditional `ev.clear()`. The **broadcast** arm: `cmCommandSetChanged` →
  `ctx.request_update_menu(self_id)` (the menu pattern).
- **Deferred + breadcrumbed (NOT stubbed):** the **keyDown global-accelerator arm**
  (deferred to the Program-wiring step — its "transform the event into evCommand
  in place and `return` WITHOUT clearing, so it propagates" semantics only make sense
  inside `getEvent`'s pre-routing; it must NOT be ported as `ctx.post`+clear, which
  double-handles); `TProgram` getEvent pre-routing + `idle()→update()`;
  `update()`/`TopView::getHelpCtx` auto-refresh (`find_items`/`set_help_ctx` ARE
  ported + tested; only the auto-trigger is deferred); the press-and-hold
  drag-highlight (`drawSelect(Some)` hover); streaming (D12); `disposeItems`/dtor (moot,
  owned `Vec`s).
- **Verification:** 25 status tests — 4 snapshots (normal+disabled, hint tail,
  narrow overflow-drop, textless-item-no-width) + bite-checked units for
  `find_items` (first-match order bite), `item_mouse_is_in` (textless-neighbour
  unaffected; col-out-of-range→None), the empty-hint skip, and both broker ends
  (broadcast arm queues `Deferred::UpdateMenu(self_id)`; the hook caches + grays).
  A full `pump_once` chain test was substituted with the two end-unit-tests (the
  `Program` test harness is `#[cfg(test)]`-private to the `program` module; the
  `Deferred::UpdateMenu → update_menu_commands` link is already covered there for
  menus) — QUALITY review judged the substitution acceptable.

## Prior session — menu MODAL layer Step-2 stage 3 (`TMenuPopup` 52) (FOUNDATION)
The **last modal piece** of the flattened `TMenuView::execute()` — standalone popup
menus — mapped onto the single `MenuSession` capture handler as three additive deltas
(no new seam). Brief: [`docs/briefs/row52-tmenupopup.md`](file:///home/oetiker/checkouts/rstv/docs/briefs/row52-tmenupopup.md)
(advisor-vetted) → Opus implementer → **full two-stage review** (SPEC then QUALITY,
fresh C++-adversarial Opus agents — both clean, no blockers) → 1 fix round →
integrate. `src/menu/menu_session.rs` + re-exports + 6 tests.

- **`put_click_event_on_exit: bool` on `MenuSession`** (THE thing that makes a popup
  not a box): the **bottom level's** `putClickEventOnExit` (`menus.h:222,229` default
  `True`; `TMenuPopup` `False`, `tmenupop.cpp:45`). Gates the single bottom-level
  exit-click re-post in `run()` (`if exit_click && self.put_click_event_on_exit`). A
  click outside a popup closes it WITHOUT re-posting; a bar still re-posts — the
  **mutual-break test pair** (`popup_click_outside_does_not_repost` vs
  `click_outside_closes_and_reposts`) proves the flag is wired, not a no-op. A
  **session-wide** flag is faithful: C++ `putEvent` is a single slot + the tail
  re-post is unconditional for any box (`parentMenu != 0`), so an exit-click in a
  deep submenu collapses to one click that rides up to the bottom frame, whose flag
  alone gates final delivery (SPEC-verified, incl. the popup→submenu→outside path).
- **`menu->deflt = 0`** (`tmenupop.cpp:51`): the popup level starts `current = None`
  AND `popup_menu` clears its **menu clone's** `default` — so the `evMouseUp`-on-margin
  arm (`menu.default.or(Some(0))`) picks the FIRST item, not a default; the box opens
  with no highlight. (A submenu opened *from* the popup keeps its own `default` — only
  the top popup zeroes, matching C++.)
- **`popup_menu(where_, menu, owner_size, ctx)` free fn + `auto_place_popup`** (faithful
  `popupMenu`/`autoPlacePopup`, `popupmnu.cpp`, via `menu_box_rect`): below-right
  placement (top-left `(p.x, p.y+1)`), desktop bottom-right clamp (`min(size, d)`),
  and the contains-`p` shift-up. Re-exported `tv::popup_menu`.
- **`end_session_with` reworked** from `skip(1)`/`first()` to a kind-keyed (`is_bar`)
  teardown loop — a popup's level 0 IS a box (must be closed), not a permanent bar
  (un-highlighted). Behaviorally identical for bar sessions (SPEC-confirmed).
- **`TMenuPopup::handleEvent` (getCtrlChar/hotKey) is MOOT and dropped** (breadcrumb,
  not stubbed): a popup is always `execView`'d, so `execute()` owns the loop and
  `handleEvent` never routes during its modal life; the accelerators are already
  covered by the flattened `step_default_key` (`find_item` on the active level +
  `hot_key(levels[0].menu)`, which for a popup IS its own tree). Only the **Ctrl+letter**
  variant is un-ported — `TODO(TMenuPopup Ctrl+letter accel)`. No persistent-insertion
  path exists in C++ (`popupMenu` is the sole ctor caller; its editor consumer is
  unported). Synchronous return value + `receiver: TGroup*` dropped (D9 async; `ctx.post`
  is the faithful `receiver->putEvent`).
- **Verification:** 6 discriminating, bite-checked tests — 3 `program.rs` `pump_once`
  (popup-opens-no-highlight, the click-outside-no-repost ANCHOR, select-command-posts),
  1 submenu-popup carry-up exit-click (the SPEC-flagged previously-only-reasoned path),
  2 `auto_place_popup` geometry units (below-right; bottom-edge shift-up). 551 lib green.

### NEXT — **`THistory` (57)** — the FOUNDATION async-modal seam (also build the **ModalFrame outside-delivery** seam row 56 deferred); then **Batch C validators 58–62** / **msgbox 63**
**Rows 54 (store) + 55 (`THistoryViewer`) + 56 (`THistoryWindow`) are DONE** (54/55
a prior session, **56 THIS session** — top git-table row). The production
`Window::insert_child`/`child_mut` seam is now built. The lowest-numbered remaining
in-sequence work is **row 57** — the FOUNDATION piece of the history cluster:

- **Row 56 discovery to carry into 57 — the `ModalFrame` outside-delivery seam.**
  Row 56 **deferred** the C++ `THistoryWindow` `evMouseDown && !mouseInView →
  endModal(cmCancel)` outside-click cancel because our `ModalFrame`
  (`src/app/program.rs`, `ModalFrame::handle`) **Consumes positional events outside
  the modal view's bounds** (a row-34 dialog simplification: delivered-and-ignored
  == swallowed, *for a dialog*). But `THistoryWindow` needs to **see** the outside
  click to cancel. Faithfully, the modal view is the top group and receives **every**
  event (C++ `TGroup::execute` `getEvent`→`handleEvent`); the modal view (a group)
  routes its *own* children positionally. So the fix is a modal-loop change:
  **ModalFrame should DELIVER outside positional events to the modal view (by id),
  not Consume them** — i.e. while a `ModalFrame` is active, route ALL events to the
  modal view rather than positionally through the root group. Design this with the
  advisor **alongside** the row-57 async-modal path (both are modal-loop work); once
  built, un-defer the `HistoryWindow::handle_event` outside-cancel arm (the breadcrumb
  is in place there).
- **Row 57 `THistory` (FOUNDATION — design with the advisor):** the dropdown-icon
  `TView` placed next to a `TInputLine` (row 39 done). On its trigger
  (`cmHistoryDropDown`/click), it **`execView`s a `THistoryWindow` from within its
  own `handle_event`** and on `cmOK` writes the picked `getSelection` string into
  the linked input line (+ `recordHistory(s)` = `history_add`). This is the
  **unbuilt D9 view-triggered async-modal path** the menu sessions reserved.
  **msgbox 63 is the co-consumer — build the seam once, wire both.**
  `THistory::draw` = the down-arrow icon (`▼`, `historyIcon`); `getPalette`
  `cpHistory`. The `link`/`historyId` fields; `shutDown` (drop the link ref, moot
  under Rust ownership).

  **Row-57 design notes (orientation gathered, NOT yet built — design with the
  advisor + main-thread care):**
  - **`exec_view` is top-level-only** (`program.rs:466`/`520`): a view holds only
    `&mut Context`, never `&mut Program`, so it cannot call `exec_view` inline from
    `handle_event`. That is the whole reason 57 needs an async seam.
  - **THE KEY CONSTRAINT (verified this session):** the pump's **deferred-apply
    phase** (`program.rs:818-`) runs inside a `let Program { group, captures,
    command_set, end_state, … } = self;` **destructure** (`program.rs:664-682`), so
    at apply-time `self` is split-borrowed — a `Deferred::OpenModal` applied there
    **cannot call `self.exec_view(view)`** (no whole `&mut self`; and it would be a
    re-entrant `pump_once` inside `pump_once`). So **do NOT plan to run `exec_view`
    at deferred-apply time.**
  - **Recommended shape (advisor to confirm):** `Deferred::OpenModal` stashes the
    boxed modal view + a completion callback into a new `Program::pending_modal:
    Option<…>` field (apply-time only touches that field — no exec_view). The
    **OUTER driver loop** (`run` / the `exec_view` loop body) checks `pending_modal`
    **after `pump_once` returns** — where it holds a whole `&mut self` — and runs
    `exec_view` on it at top level, then invokes the completion callback with the
    result `Command` + access to the modal view (for `get_selection`, already built
    on `HistoryWindow`) + a `Context` to write the result back. Keeps `exec_view`
    top-level; no re-entrant destructure.
  - **Result flowback into the link `TInputLine` (row 39):** `THistory` holds only
    the link's `ViewId` (D3), so writing the picked string back is a deferred/broker
    write (set the input line's data by id — likely a new `Deferred` variant or the
    D10 `set_value` path) + `link.select_all` + redraw. Check `src/widgets/`
    input_line for the existing data setter before inventing one.
  - **ModalFrame outside-delivery** (the row-56-deferred seam, above) is the *other*
    modal-loop change 57 owns — design both together; they share the loop.
- **Batch C concrete validators 58–62** (`tvalidat.cpp`) — the clean worktree
  parallel fan-out (see "Available parallel fan-out" below); **59 `TRangeValidator`
  is FOUNDATION-ish** (resolves the deferred `transfer` hook + the `cur_pos`
  re-clamp hazard; `FieldValue::Int` ready). **Available NOW** (independent of the
  history seams) if you'd rather fan out than design the async-modal path.
- **msgbox 63** — co-consumer of the row-57 async-modal seam (`Deferred::OpenModal`
  + posted completion `Command`); uses the now-built production `Window::insert_child`
  (row 56's seam) to add its `TStaticText`/`TButton`/`TInputLine` children.
- **`cmDosShell`** is still deferred — needs a backend terminal-suspend seam + SIGTSTP.

Other open follow-ons (lower priority / parallel):
- **idle→`statusLine->update()` help-ctx refresh** — still deferred; only worth doing
  when a **context-split `OneOf` `StatusDef`** lands (under a single `All` def it is
  inert). Would need a `View::get_help_ctx` method (+ a `tvision-macros/specs.rs`
  forwarder) + a `TopView` resolver (nearest `sfModal` view = the top capture's view,
  else the root group).
- **status-line press-and-hold drag-highlight** (`drawSelect(Some)` hover) —
  `TODO(row 31, D9)`.
- **`program_handle_event` modal-isolation** breadcrumb (suppress program-level
  interception while a `MenuSession`/modal is active) and the `ModalFrame`/`DragCapture`
  "(0,0)-desktop absolute-coords" caveats (the bar now shifts the desktop down by 1 —
  re-examine when a dialog must position relative to the desktop, not the screen).

Batch C concrete validators 58–62 (`tvalidat.cpp`) remain an available parallel fan-out.

## Prior session — menu MODAL layer Step-2 stage 2 (MOUSE nav) (FOUNDATION)
The **mouse** arms of the flattened `TMenuView::execute()`, layered onto the stage-1
`MenuSession`. Brief: [`docs/briefs/row50-52-menu-modal-mouse.md`](file:///home/oetiker/checkouts/rstv/docs/briefs/row50-52-menu-modal-mouse.md)
(advisor-vetted) → Opus implementer → **full two-stage review** (SPEC then QUALITY,
fresh C++-adversarial Opus agents) → 2 fix rounds → integrate. `src/menu/menu_session.rs`
+ `src/menu/menu_view.rs` + 10 tests in `src/app/program.rs`.

- **One shared `run()` loop.** Stage-1's `handle_key` is refactored into a single
  `run(ev, ctx)` that dispatches keyboard/mouse/cmMenu **steps** into the *same*
  post-switch tail (set-current → `last_target_item` reset → open-gate →
  doReturn-pop/re-apply). Keyboard behaviour is preserved (verified: all stage-1
  tests still green); the mouse arms reuse the cross-level re-apply unchanged.
- **Coordinate model:** positions + `MenuLevel::bounds` are **root-frame** (the
  session sees events pre-translation via the capture stack — no `makeLocal`);
  `item_rect_global = item_rect_local` shifted by `level.bounds.a`. New helpers
  `track_mouse` (overwrites `current` to the hit item or `None`; sets `mouse_active`
  monotonically), `mouse_in_view`/`mouse_in_owner` (parent's current-item rect) /
  `mouse_in_menus` (any **parent** level's bounds).
- **Per-level loop-locals added** to `MenuLevel`: `last_target_item`, `mouse_active`,
  `first_event` (each C++ `execute()` per-frame, re-init per level — never leak).
- **THE crux** (`tmnuview.cpp:383-386`, advisor-flagged): `lastTargetItem = current`
  **+ `menu->deflt = current` + `firstEvent = False`** are set on the **parent** at
  the **child-pop** point (the flattened "execView returns") — this is what makes
  **clicking an already-open title CLOSE it** (re-applied click hits the bar:
  `autoSelect = !current || lastTargetItem != current` → `File==File` → False → gate
  shut). Bite-tested.
- **Open-gate re-applies the carried event into a freshly-opened child only for
  `MouseDown`/`MouseMove`** (C++ `putEvent` gating `e.what & (evMouseDown|evMouseMove)`);
  keyboard + `MouseUp` open-and-wait. A box **never** sets `autoSelect` (only the bar
  does, `:273`) — so a nested submenu opens only via `MouseUp`-`doSelect`, not
  drag-hover (a SPEC-confirmed brief correction).
- **Click-outside-closes + re-post** happens at the **bar** level only
  (`putClickEventOnExit`, `:217`); a box's exit-click re-applies up the stack and the
  bar does the final `ctx.put_event` so the view under the click recovers focus.
- **`evMouseDown` bar activation** (`do_a_select`, `:505`) in `menu_view::handle_event`
  (replaces the stage-1 breadcrumb): gated `size.y==1 && bounds.contains(position)` →
  `menu_session::activate_mouse` pushes the bar-only session and **re-posts the click**
  (no pre-open); the session's evMouseDown arm + open-gate then open the clicked title.
- **`cmMenu` routed through `run()`** (SPEC fix, `:343-350`): a box-level cmMenu now
  `doReturn`s (closes the box, re-applies up) and the bar resets `autoSelect`/
  `last_target_item` + stays open — was previously a top-only reset that left a box open.
- **Two-stage review earned its keep:** SPEC **independently confirmed 3 brief-was-wrong
  deviations correct vs the C++** (bar-click leaves the dropdown *unhighlighted* until
  the mouse enters it; a box never auto-selects; a test-bite re-target) and caught the
  cmMenu-closes-box divergence (fixed). QUALITY found no blockers and closed **two
  uncovered `evMouseUp` arms** (release-on-parent-title→reset-to-default;
  release-outside-after-activating→close) with bite-checked tests + fixed an inaccurate
  `track_mouse` comment.
- **Verification:** 10 discriminating, bite-checked `pump_once` tests (click-opens-box,
  click-open-title-closes [the crux], drag-to-neighbour-reopens, click-outside-closes+
  reposts, drag-into-submenu, mouseUp-on-command-posts, mouseUp-on-box-margin-resets,
  cmMenu-from-nested-box-closes-to-bar, mouseUp-on-parent-title-resets, mouseUp-outside-
  after-activating-closes). 545 lib tests green.

## Prior session — menu MODAL layer Step-2 stage 1 (keyboard nav) (FOUNDATION)
The interactive `TMenuView::execute()` (`tmnuview.cpp:179`), flattened onto our single
event loop (D9). Brief: [`docs/briefs/row50-52-menu-modal.md`](file:///home/oetiker/checkouts/rstv/docs/briefs/row50-52-menu-modal.md)
(advisor-vetted **twice** — architecture then concrete mechanism) → Opus implementer →
**full two-stage review** (SPEC then QUALITY, fresh C++-adversarial Opus agents) → two
fix rounds → integrate. `src/menu/menu_session.rs`.

- **THE ARCHITECTURE DECISION (settled, do not relitigate).** C++ `execute()` is a STACK
  of nested `execView` modal loops (opening a submenu *recurses* `owner->execView`). Two
  mappings were weighed and **the advisor + C++ evidence killed the re-entrant one:**
  - **REJECTED — re-entrant `exec_view` per level.** The guide's "`exec_view` = the
    `TGroup::execute` shape" ratifies `exec_view`/`OpenModal` for **`TGroup::execute`**
    (`tgroup.cpp:173`, the *dialog* loop). `execView` calls `p->execute()` **virtually**
    (`tgroup.cpp:205`); for a menu, `p` runs the **overridden `TMenuView::execute`**
    (`menus.h:152`) — a *different function*. So the guide reserved `OpenModal` for "a
    menu *command* launches a dialog," and **never licensed it for menu nav.** Also
    `ModalFrame` *swallows* outside clicks (menus must *close* on them) and per-level
    bounds-gating can't express cross-level mouse (`mouseInMenus`/`mouseInOwner` walk the
    whole `parentMenu` chain). **My initial lean conflated the two `execute()`s — caught
    by the advisor.**
  - **CHOSEN — one `MenuSession` capture handler** owning the WHOLE open stack (bar + every
    open box), the flattened `execute()`. Clean Architecture A: while active it **consumes
    every event**; bar + boxes are **display-only** (never focused). `OpenModal`/`exec_view`
    stays reserved for the menu-command→dialog case (msgbox / Batch E).
- **The flattening insight (the "beautiful" part).** C++'s `doReturn` pops a level and
  **re-posts the triggering event** to the parent's `getEvent` **unless that arm cleared
  it** (`tmnuview.cpp:401-405`). Flattened: `MenuSession::handle_key` is a **re-apply-across-
  levels loop** — on a non-cleared `doReturn`, pop the level and re-deliver the SAME event
  to the new top. This one mechanism produces all the cross-level keyboard behaviors:
  one-Esc-closes-the-whole-menu (from a 1st-level box), Esc-closes-one-level (from a 2nd+
  box), and Left/Right unwinding the stack to the bar + walking to the neighbour title.
- **State:** `MenuSession { levels: Vec<MenuLevel>, owner_size }`; each `MenuLevel {
  view_id, menu (clone-at-open), current, bounds, is_bar, auto_select }`. **Clone-at-open
  is FAITHFUL** (execute() has no `evBroadcast` case → `disabled` frozen for the menu's
  lifetime; the session **swallows broadcasts** while active). **`auto_select` is a keyboard
  concern** (not mouse-only): set on bar kbDown/kbEnter/alt-activation, reset by `cmMenu`;
  it drives the Left/Right title-walk re-open. Bounds cached at open (a box never moves →
  no `sync_gate_bounds`); shaped for stage-2 mouse.
- **New seams (all additive — "a new deferred capability ADDS A VARIANT"):**
  `Deferred::OpenMenuBox { id, menu, bounds }` (the session **pre-mints** the id via
  `ViewId::next`, the pump `Group::insert_with_id`s the box into the root group, **no focus
  move**) + `Deferred::SetMenuCurrent(id, Option<usize>)` (write-only highlight cache, via
  the new defaulted `View::set_menu_current` trait hook — no downcast, mirrors
  `update_menu_commands`; forwarder added to `tvision-macros/specs.rs`) + `ctx.put_event`
  (raw-event sibling of `post`, ports `putEvent`).
- **Activation** (replaces the row-49 `_ => {}` breadcrumb in `menu_view::handle_event`):
  bar `cmMenu`/kbF10 → highlight the default title, **no box** (F10 waits — the `autoSelect=
  False` path); bar **alt-shortcut** → open the matched submenu's box (or post directly if
  it's a top-level command). Pushes the session + the first `OpenMenuBox` in the **same
  deferred batch** (no dead first event).
- **Two-stage review earned its keep (twice over):** SPEC caught 3 keyboard-faithfulness
  blockers (F10-wrongly-opens-box; one-Esc should close the whole menu; Left/Right should
  walk+reopen → `autoSelect` is keyboard, not mouse). QUALITY caught a **real bug SPEC
  missed** — a hotKey accelerator pressed while a box is open was *silently dropped* instead
  of closing the menu + firing the command (the unreachable "defensive" branch was the
  tell) — plus a dead/semantically-broken `first_event` field and 3 clean wins (shared
  `matching_item` helper, stale doc, untested `put_event` path).
- **Verification:** 11 discriminating, bite-checked `pump_once` integration tests
  (F10-no-box, arrow-move, submenu-recurse, command-post+close, Esc 1st-vs-2nd-level
  asymmetry, Right-walk-reopen, F10-then-Right-no-box, alt-shortcut-at-matched, hotKey-
  accelerator-closes-whole-menu, foreign-command-close+repost). 535 lib tests green.

## Prior session — Rows 50/51 `TMenuBar`/`TMenuBox` DRAW/DATA layer (FOUNDATION)
The **draw/data slice** of the menu views — drawing + geometry + the polymorphism
seam, **no modal loop**. Brief: [`docs/briefs/row50-51-menu-draw.md`](file:///home/oetiker/checkouts/rstv/docs/briefs/row50-51-menu-draw.md)
(advisor-vetted scope split) → Opus implementer → **full two-stage review** (spec
then quality, fresh C++-adversarial Opus agents, both PASS) → 6 polish items (M1
`MenuColors` unification + M2/N1 clarity + T1/T2/T4 edge-case tests) → integrate.

- **The scope split (advisor-confirmed, overrides the old "50/51/52 land together"
  framing):** `draw`/`getItemRect`/`getRect` read only `menu` + `current` — never the
  modal state — so the draw layer is a clean, snapshot-testable slice. The modal
  `execute()` loop, `TMenuPopup`, navigation, and the D9 async-modal path are a
  **separate Step-2 design session** (see NEXT). Landing tested draw first de-risks
  the modal work (verified substrate to navigate) — the HANDOVER itself conceded this
  ordering ("each menu view needs getItemRect + draw *so* execute()'s nav is testable").
- **The `MenuView` trait** (`src/menu/menu_view.rs`) — row 49's "no trait yet" decision
  **flips** here: `get_item_rect`/`draw` ARE the overridable virtuals (bar ≠ box). Mirrors
  the row-28 `ListViewer` shape: trait `MenuView: View` with `mv()/mv_mut()` accessors +
  defaulted `get_item_rect(index) -> Rect` (base = empty rect, C++ `TRect(0,0,0,0)`); the
  passive `hot_key`/`update_menu_commands`/`handle_event` stay as the row-49 **free fns**.
  `mv()/mv_mut()` are unused now (the Step-2 polymorphism seam; reachable as pub-API trait
  items so no `dead_code`).
- **`MenuViewState.current: Option<usize>`** added (index into `menu.items`; `None` == C++
  `current == 0`; consistent with `Menu::default`). Verified against every Step-2
  `execute()` mutation (it fits all). **`parentMenu` still deferred** — draw/getItemRect
  never read it; only the Step-2 modal-nav methods do.
- **`TMenuBar`** (`src/menu/menu_bar.rs`): `draw` (`tmenubar.cpp:48` — bg fill + left-to-right
  items, the `if x+l<size.x` clip with `x += l+2` advancing even when clipped, the 4-color
  matrix), `get_item_rect` (horizontal accumulator, separators consume no x), ctor sets
  `gfGrowHiX` + `ofPreProcess`. **`handle_event` delegates to the row-49 passive
  `menu_view::handle_event`** (C++ `TMenuBar::handleEvent` *is* `TMenuView::handleEvent`,
  not overridden) — so row 49 finally has a concrete consumer.
- **`TMenuBox`** (`src/menu/menu_box.rs`): the `menu_box_rect` sizing helper (`getRect`,
  `tmenubox.cpp:25`), `frame_line` (the `frameChars` table decoded to single-line box glyphs
  from `Glyphs` — `frame_tl/tr/bl/br/h/v/tee_l/tee_r`; **note the faithful inset: cols 0 and
  size.x-1 are blank**), `draw` (`tmenubox.cpp:80` — top border → one row per item → bottom;
  per-line `color` fill split from `cNormal` borders; submenu `►` at size.x-4; param
  right-aligned at size.x-3-cstrlen), `get_item_rect` (y from 1). Ctor sets `sfShadow` +
  `ofPreProcess`. `handle_event` delegates to the passive layer (TMenuBox inherits it).
- **Theme:** 6 `Role` variants for the `cpMenuView` palette (`MenuNormal`/`…Shortcut`/
  `MenuSelected`/`…Shortcut`/`MenuDisabled`/`MenuSelectedDisabled`, idx 1/3/4/6/2/5).
  Disabled roles use one style for both lo+hi (no shortcut highlight when greyed).
  **Colours provisional** — `TODO(row 34 gray theming)`. Spec review resolved the faithful
  `cpAppColor` bytes (the row-34 realignment target): cNormal lo=`0x70` hi=`0x74`,
  cSelect lo=`0x20` hi=`0x24`, cNormDisabled=`0x78`, cSelDisabled=`0x28` (4 of 6 seeds are
  already exact; the 2 selected-fg seeds are brightened, realign with the other provisional
  Input/Scroller colours as one coherent pass).
- **`MenuColors`** (`menu_view.rs`, pub) — the 4 `(lo,hi)` pairs + `resolve(&DrawCtx)` +
  `.item(disabled, selected)`; shared by bar AND box (killed an 8-arg helper + its
  `#[allow(too_many_arguments)]`).
- **Verification:** 2 snapshots (bar highlight+disabled; box frame+highlight+disabled+
  separator+param+submenu) + a 3rd narrow-bar snapshot (clip-skip branch) + bite-checked
  unit tests for `get_item_rect` (bar+box) and `menu_box_rect` sizing (incl. a discriminating
  submenu-`+3` test) + empty-menu no-panic + a `handle_event` accelerator-delegation smoke.
  `cargo-insta` NOT installed → `.snap`s generated via `INSTA_UPDATE=always`, hand-verified,
  committed.

_(The Step-2 modal-layer plan that previously lived here was **executed this session** —
its capture-stack hypothesis was advisor-refined into the `MenuSession` architecture and
the re-apply loop; see **What landed THIS session** + the **NEXT** section above, and the
brief [`docs/briefs/row50-52-menu-modal.md`](file:///home/oetiker/checkouts/rstv/docs/briefs/row50-52-menu-modal.md).)_

## Prior session — Row 49 `TMenuView` passive layer (FOUNDATION)
The **passive (non-modal) layer** of `TMenuView` (`tmnuview.cpp`): command-graying
+ hotkey-accelerator dispatch, **no drawing / no modal loop** (those are 50–52).
`src/menu/menu_view.rs`. Built main-thread/Opus-orchestrated: advisor-vetted brief
(`docs/briefs/row49-tmenuview.md`) → Opus implementer → **full two-stage review**
(spec then quality, fresh C++-adversarial Opus agents, both PASS) → one MINOR
doc-link fix → integrate. **Scope was deliberately split** (advisor-confirmed): the
interactive `execute()` modal loop maps to the *unbuilt* D9 view-triggered
async-modal path and lands with the drawing subclasses.

- **Command-graying = a BROKER, NOT a `Context` read-accessor** (this **overturned**
  the prior HANDOVER note that said "add a read accessor on `Context`"). The command
  set lives on `Program`; the pump's apply-phase `Context` is alive across a loop
  whose `EnableCommand`/`DisableCommand` arms mutate `command_set` (`&mut`), so a
  `&CommandSet` on `Context` would alias that borrow (+ would add a `Context::new`
  param at every call site). Instead: new **`Deferred::UpdateMenu(ViewId)`** +
  **`Context::request_update_menu`** + defaulted **`View::update_menu_commands(&mut
  self, &CommandSet)`** (no-op default), applied in the pump where `group` and
  `command_set` are **disjoint destructured fields** (no `ctx` needed). The exact
  `Deferred::SyncListViewer` + `View::apply_list_scroll` precedent — *a new deferred
  capability ADDS A VARIANT*. Delegate forwarder added to `tvision-macros/specs.rs`
  + the `delegate_view` spy test (count 21→22).
- **`updateMenu` ported** as `menu_view::update_menu_commands(&mut Menu, &CommandSet)`:
  recurse submenus, `disabled = !cs.has(command)` on command items only (never a
  submenu's own flag), skip separators. The C++ `Boolean` return is **dropped** (D8
  whole-tree redraw makes `if updateMenu drawView` moot; the guarded write collapses
  to an unconditional flip).
- **`hotKey`/`findHotKey` ported** as `menu_view::hot_key(&Menu, KeyEvent) ->
  Option<Command>`: depth-first, skip separators, recurse submenus **regardless of
  the submenu's own `disabled`** (C++ `!disabled` guard is only on the command
  branch), match a command item iff `!disabled && key_code == Some(key)`. The passive
  `evKeyDown` handler posts the matched command. **The C++ `commandEnabled(p->command)`
  re-check is dropped** — safe because (a) the cached `disabled` is kept current by
  the broker and `hot_key` already filters it, and (b) the pump's `drop_disabled`
  boundary filter drops a stale-enabled post; only a one-idle-cycle staleness window
  remains (documented).
- **evBroadcast mask is MOOT** — `Group::handle_event` fans broadcasts to **every**
  child unconditionally (test `broadcast_reaches_all_children_including_disabled`), so
  the C++ `eventMask |= evBroadcast` opt-in needs no port; no gate added.
- **`MenuViewState { state, menu }`** is the embed target for 50/51. **No `MenuView`
  trait yet** and **`current`/`parentMenu` omitted** (omit-until-consumer: only
  `execute()`/`trackMouse`/`getHelpCtx` use them — added with the modal layer at
  50–52). Free functions, not a trait, since the passive layer dispatches into no
  overridable virtual.
- **Deferred + breadcrumbed (NOT stubbed):** `execute()` (the nested modal loop →
  D9 `OpenModal`), `trackMouse`/`trackKey`/`nextItem`/`prevItem` (modal nav),
  `findItem`/`findAltShortcut`, `do_a_select`/`newSubView`/`mouseInOwner`/
  `mouseInMenus`/`topMenu`, `getItemRect`/`draw`/`getPalette` (`cpMenuView`),
  `getHelpCtx`, streaming (D12). The activation branches of `handle_event`
  (`evMouseDown`, `cmMenu`, alt-shortcut) are breadcrumbed (leave the event live).
- **Verification (no snapshot — nothing draws):** 8 unit tests on `hot_key` (submenu
  recursion, disabled-skip bite, separator/no-key, submenu-own-key-no-match) +
  `update_menu_commands` (recursive regray, negated-predicate bite, submenu-flag
  untouched); **2 integration tests** through real `pump_once` — a `#[cfg(test)]
  MenuProbe` (FakeList precedent) proving the broker end-to-end (enable→regray→enabled,
  disable→`cmCommandSetChanged`→request→apply→disabled, bite-checked) + the
  accelerator-post path (enabled posts, regrayed-disabled posts nothing).

_(The Step-2 modal-layer plan that previously lived here is now the **NEXT** section
above — updated with the capture-stack-not-`OpenModal` framing + the carried
initial-regray gap.)_

## Prior session — Row 46 `TMenu` data tree (FOUNDATION)
First Phase-4 row: the **menu data tree** (`TMenuItem`/`TSubMenu`/`TMenu`,
`menus.h`/`menu.cpp`) — pure data + a builder, **no `View`** (that's row 49).
`src/menu/mod.rs`, wired into `lib.rs` (`pub use menu::{Menu, MenuBuilder,
MenuItem}`). Built main-thread/Opus-orchestrated: brief
(`docs/briefs/row46-menu-data-tree.md`, advisor-vetted design) → Opus implementer
→ **full two-stage review** (spec then quality, fresh C++-adversarial Opus agents,
both PASS) → doc-only fixes → integrate.

- **Data model = a 3-variant enum** (`MenuItem::{Separator, Command{…},
  SubMenu{…}}`), the type-safe translation of the C++ `union { param; subMenu }`
  discriminated by `name==0`⇒separator / `command==0`⇒submenu / else command.
  Shared fields (`name`/`key_code`/`help_ctx`/`disabled`) read via or-patterns;
  **no speculative common sub-struct** (advisor: add it later iff 49–52 want it).
  `MenuItem::disabled_mut() -> Option<&mut bool>` (None for `Separator`) for the
  row-49 command-graying loop.
- **`Menu { items: Vec<MenuItem>, default: Option<usize> }`** — C++ linked list
  `next` → `Vec`; `deflt` pointer → an **index**. The builder sets `default =
  Some(0)` on first push (C++ `TMenu(itemList)` head, no separator-skip), `None`
  when empty; both fields are `pub` and the two-arg C++ `TMenu(itemList, deflt)`
  allows a non-head default, so `default` is documented as *any valid index*.
- **`key_code: Option<KeyEvent>`** (None == C++ `kbNoKey`, faithful to the
  decomposed key model = absence of a key event); **`param: Option<String>`**
  (None == C++ `param==0`; empty `""` → `None`).
- **Builder replaces C++ `operator+`** (`MenuBuilder`: `.separator()`,
  `.command(name,cmd)`, `.command_key(name,cmd,key,param)`,
  `.submenu(name,key,|m| …)` closure-nested, `.item(MenuItem)` raw escape hatch).
  Local `fn alt(char) -> KeyEvent` convenience (mirrors `kbAltX`; `key.rs`
  untouched).
- **Verification is NOT a snapshot** (pure data, renders nothing): the lead test
  builds the canonical File/Window menu via the builder and `assert_eq!`s it
  node-for-node against a hand-built literal tree (a *different* code path, so a
  builder bug can't pass silently) + 5 edge-case tests. **6 tests, all pass.**
- **Scope fenced (FOUNDATION-creep guard):** no `View`/draw/event/`execute`/
  `findItem`/`hotKey`/`getItemRect`/streaming — all rows 49–52.

## Prior session — Row 32 `TApplication` (`3e6645f`, MECHANICAL)
The thin D2 embed wrapper over `Program` (row 31): `Application { program: Program }`,
the type a real app constructs. **Genuinely thin by dependency order**
(advisor-confirmed) — all of `TApplication`'s substance is deferred, so the row is
the type + one real body + faithful breadcrumbs, deliberately NOT padded. Built
main-thread/Opus-orchestrated: tight brief (`docs/briefs/row32-tapplication.md`) →
Sonnet implementer (in a `/scratch` worktree) → spec review (fresh C++-adversarial
agent) → fixes → integrate.

- **`Application`** forwards `run`/`pump_once`/`exec_view`/`desktop`/`end_modal`/
  `end_state`/`{enable,disable,command_enabled}_command` + `program()`/`program_mut()`
  escape hatches — hand-written one-liners. **(Note: `#[delegate]` does NOT apply
  here** even though it later landed and was adopted everywhere — that macro
  generates the `View`-trait forwarding impl for D2 embeds; `Application` forwards
  `Program`'s *inherent* loop methods, not the `View` trait, so it stays
  hand-written. It is correct as-is.)
- **`get_tile_rect()` is the one real body** → new **`Program::get_tile_rect`**
  (the desktop child's extent = `deskTop->getExtent()`, local-origin `(0,0,w,h)`,
  `None` if no desktop; `&mut self` because `Group::find_mut` is `&mut`). Placed on
  `Program` (not `Application`) because `Application` can't reach the private `group`,
  and the future command handler — also in `Program` — reuses it.
- **Deferred (NO dead stubs — omit-until-consumer, the row-35/48 rule):**
  `tile`/`cascade` (need `Desktop::tile`/`cascade` geometry [`mostEqualDivisors`/
  `iSqr`/`calcTileRect`/`dividerLoc`/`doCascade`, `tdesktop.cpp`] + a menu to emit
  cmTile/cmCascade + a way to test → Phase 4); `dosShell`/`suspend`/`resume` (need a
  backend terminal-suspend seam + SIGTSTP); `initHistory`/`doneHistory` (history
  subsystem unported); `TAppInit` subsystem init **dropped** (subsumed by the
  `Backend`/`Renderer` construction path).
- **Command handling breadcrumbed, not wired:** `TApplication::handleEvent`'s
  cmTile/cmCascade/cmDosShell are **program-level** → a TODO in `program_handle_event`
  **after** `group.handle_event` (faithful: C++ runs `TProgram::handleEvent` first),
  beside the QUIT catch. Blocked on the deferred bodies. The consts
  `Command::{TILE,CASCADE,DOS_SHELL}` already exist + are enabled in
  `default_command_set`, but **nothing emits them yet (no menus)** — Phase 4 menus
  are the first emitters; when they land, wire this breadcrumb + build the desktop
  geometry together (trigger + body + test in one go).
- **Review caught + fixed a BLOCKER:** the implementer first added empty
  `tile`/`cascade`/`dos_shell` methods on `Application` — dead stubs (the planned
  handler is in `program_handle_event`, which can't reach `Application`); deleted,
  deferral kept in docs + the breadcrumb. Plus 2 MINORs fixed: breadcrumb moved
  post-dispatch; the `get_tile_rect` test made discriminating (inset 80×20 desktop on
  an 80×25 backend pins desktop-extent vs screen-extent — a screen-rect impl fails it).

## Also landed — the `#[delegate]` proc-macro (`47894f0`…`66ab55f`)
The D2 embed-and-delegate pattern (`Wrapper { inner: Inner }` re-implementing the
whole `View` trait by forwarding to `inner`) was hand-written boilerplate in every
wrapper (Dialog→Window, the cluster family, etc.). It is now a proc-macro:
**`#[delegate(to = <field>)]`** in the new **`tvision-macros`** crate (a workspace
member; the repo root is now a Cargo workspace `["tvision-macros"]`). Applied to a
struct, it generates the `View`-trait forwarding `impl` to the named field.

- **Adopted codebase-wide**, replacing the hand-rolled forwards and the
  `cluster_wrapper!` macro: `cluster` (`2a715a0`), `Window` (`c357c3a`, `to=group`),
  `Dialog` (`e4eaad3`, `to=window`), `ParamText` + `Label` (`be70841`), `Desktop`
  (`7e90907`, `to=group`), and the `hello` example's `AboutDialog` (`415edb8`,
  `to=dialog`).
- **Spec + test:** a "full `View` forwarder spec" with a behavioral spy test
  (`4d92646`) → new integration test **`tests/delegate_view.rs`** (the +2 in the
  build-state count); code-review fixes for docs/diagnostics/drift-signposts
  (`375ef03`); a design note + a CLAUDE.md convention (`30cfe1f`).
- **Implication for future D2 wrappers:** prefer `#[delegate(to = inner)]` over
  hand-writing the `View` forwards. It applies when the wrapper forwards the **`View`
  trait** to an embedded `View` field; it does NOT apply to inherent-method forwards
  (e.g. `Application`→`Program` loop methods). When you override a method (the
  wrapper's own `handle_event`/`valid`), keep that method and let the macro forward
  the rest — check the macro's drift-signpost docs for the override pattern.

### Prior session — Row 48 `TListBox` (`fc66637`, MECHANICAL)
The first **concrete** `TListViewer`, proving the row-28 trait seam end to end.
Built main-thread/Opus-orchestrated: tight brief
(`docs/briefs/row48-tlistbox.md`) → Sonnet implementer → full two-stage review
(SPEC then QUALITY, fresh C++-adversarial Opus agents) → integrate.

`ListBox { lv: ListViewerState, items: Vec<String> }` (`src/widgets/list_box.rs`)
reuses **all** of `TListViewer`'s draw/event/nav verbatim via the `ListViewer`
trait, overriding **only `get_text`** (`items.get(item as usize).cloned().
unwrap_or_default()` — collapses the C++ `items==0→EOS` + OOB cases, panic-free);
`is_selected`/`select_item` **inherit the base** (C++ overrides neither). `impl
View` delegates `draw`/`handle_event`/`set_state`/`cursor_request`/
`apply_list_scroll`/`as_any_mut` to the `list_viewer::*` free fns (the `FakeList`
template). Wired into `widgets/mod.rs` + `lib.rs`.

- **D10 value protocol — first consumer beyond `TInputLine`:** `value() →
  FieldValue::Int(focused)` (the `getData` selection half; the collection is
  config `new_list` manages, NOT part of the transferable value — no `List`
  variant, `FieldValue` grows per consumer).
- **`set_value` DEFERRED** (advisor-confirmed): the **`Context`-free**
  `View::set_value` signature can't republish the v-bar (C++ `setData` =
  `newList`+`focusItem`, both need a `Context` in our model), so a partial would
  leave the scroll thumb desynced after a scatter. Lands with the **dialog
  gather/scatter** consumer (inputBox/Batch E), which must itself solve threading
  a `Context` into scatter. `TODO(set_value: dialog gather/scatter)`.
- **Population is post-insert** (the ctor has no `Context`): `new_list(items,
  ctx)` (`set_range` + `focus_item(0)` iff `range>0`) **plus**
  `list_viewer::update_steps(ctx)` for the page/arrow steps — miss either and the
  thumb starts unsynced. Documented on the type.
- **Dropped:** `dataSize`/`TListBoxRec` (→ typed value), streaming (D12),
  `drawView` (D8). The dialog gather/scatter group-walk stays deferred (no
  consumer yet).
- **Process catch — out-of-scope creep reverted:** the implementer also added an
  exported `delegate_view_rest!` macro to `src/view/view.rs` + refactored
  `examples/hello.rs` to use it — unrelated to row 48, unreviewed (both review
  agents were scoped to `list_box.rs`), and touching a FOUNDATION file. Reverted
  before commit. The macro is a genuinely useful D2-embed helper; if wanted, do it
  deliberately as its own reviewed change.

### Prior session — Row 28 `TListViewer` (`c1ad789`, FOUNDATION)
`TListViewer` (base for `TListBox` 48, history, color/file lists) drives two
sibling scrollbars like `TScroller` but **diverges structurally in two ways** the
"reuse the broker verbatim" line glossed over — both confirmed with the advisor
*before* building. Built main-thread/Opus: brief → Opus implementer → two-stage
review (SPEC then QUALITY, fresh C++-adversarial agents) → fixes. Brief:
`docs/briefs/row28-tlistviewer.md`.

**Divergence 1 — `ListViewer` is a TRAIT, not a concrete struct (the `Validator`
pattern, NOT the `Scroller` embed shape).** `TListBox` reuses `TListViewer::draw`
while *overriding* the virtuals `getText`/`isSelected`; a D2 concrete-embed base
physically cannot dispatch back into the embedder's `getText` from the base's own
`draw`. So:
- `ListViewer: View` trait — `lv()`/`lv_mut() -> &ListViewerState` accessor +
  defaulted `get_text`/`is_selected`/`select_item`.
- `ListViewerState` struct holds the data members (`state: ViewState`, `num_cols`,
  `top_item`, `focused`, `range`, `indent`, `h_scroll_bar`/`v_scroll_bar` ids).
- The shared draw/event/nav logic lives as **free functions generic over
  `<L: ListViewer + ?Sized>`** (`list_viewer::draw`/`handle_event`/`focus_item`/
  `focus_item_num`/`set_range`/`update_steps`/`apply_scroll`/`set_state`/
  `focused_cursor`), which a concrete widget's `View` impl calls.
- Object-safety: `ListViewer` is **not** object-safe (`get_text -> String`) — fine,
  it's only ever a generic bound; concrete widgets are still `Box<dyn View>`.
- A `#[cfg(test)] FakeList` (Vec-backed) is the first consumer (a real consumer for
  the draw/nav tests, NOT a dead stub). **Row-48 `TListBox` is the production one.**

**Divergence 2 — the read-sync WRITES BACK (the scroller never did).** C++
`focusItem → vScrollBar->setValue(item)`; in our model the read-sync issues a
deferred `ScrollBarSetParams{value}`. New mechanism, **scroller path untouched**:
- New defaulted-no-op **`View::apply_list_scroll(&mut self, h, v, ctx)`** + new
  **`Deferred::SyncListViewer{list,h,v}`** + a pump apply arm that calls the **trait
  method (NO downcast** — you can't cast `dyn View → dyn ListViewer`, unlike the
  scroller's `as_any_mut` downcast to a single concrete type).
- **TERMINATION (the centerpiece property):** the vbar→sync→setValue cycle
  terminates **only because `ScrollBar::set_params` is change-guarded**
  (`scrollbar.rs:219/224` — broadcasts `SCROLL_BAR_CHANGED` iff `old_value !=
  a_value`), so the write-back of the already-current value is a silent no-op.
  Proven by a discriminating termination test through real `pump_once` drains
  (6 passes asserting quiescence; bite-checked — removing the guard makes it spin).
- **`indent` cached** on `ListViewerState`: draw can't read the sibling hbar live,
  so the hbar `value` is cached and refreshed by the same sync (the hbar
  `cmScrollBarChanged` branch, C++ "just drawView", becomes "update the cache").

**Reused verbatim from row 27:** `Deferred::ScrollBarSetParams` (setRange +
ctor-setStep) and `SetVisible` (setState show/hide), `Broadcast{source}` as the
`source ∈ {h,v}` filter, `View::value() → FieldValue::Int`.
- **`setState`** uses the C++ **`active && visible` AND-condition** for show/hide
  (NOT the scroller's `active || selected` — a spec-review crosshair).
- **`cmScrollBarClicked` from an own bar → `select()`** → `ctx.request_focus(id)`
  (the row-41 `Deferred::FocusById` seam).
- **Theme reconciled** to the 5-entry cpListViewer palette (`Active/Inactive/
  Focused/Selected/Divider`) → roles `ListNormalActive`/`ListNormalInactive`/
  `ListFocused`/`ListSelected`/`ListDivider` (the old guessed `ListNormal`/
  `ListSelectedFocused` were unused; provisional colours, `TODO(window-scheme
  remap)`).
- **Deferred + breadcrumbed:** mouse press-and-hold/auto-scroll `do…while
  (mouseEvent)` loop (`TODO(row 31, D9)`; ship single-shot + double-click select);
  `changeBounds` step republish (`TODO(resize)` — **note the distinct formula**:
  C++ `changeBounds` uses vbar plain `size.y` + **both bars preserve arStep**,
  unlike the ctor's `update_steps`; do NOT call `update_steps` for resize —
  corrected in-doc after a spec catch); `showMarkers` + streaming dropped (D8/D12);
  scroller/listviewer read-sync unification noted optional/out-of-scope.

### Prior session — Row 27 `TScroller` (`543b2c8`, FOUNDATION)
Established THE cross-view scrollbar broker (pump brokers all scroller↔scrollbar
reads/writes at deferred-apply via `group.find_mut(id)` + `as_any_mut`/
`View::value()`; `Broadcast{source}` is the filter, value NOT stuffed into the
message). New `Deferred`: `SyncScrollerDelta` (read → `apply_delta`),
`ScrollBarSetParams` (write, per-field `Option`=preserve), `SetVisible`. New seams
`FieldValue::Int` + `ScrollBar::value()`. Dropped (D8) `drawLock`/`drawFlag`/
`checkDraw`/`drawView`. `Role::ScrollerSelected` + `changeBounds` resize-republish
deferred to `TEditor` 66. Brief: `docs/briefs/row27-tscroller.md`.

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

### Phase-4 breadcrumb from Row 32 `TApplication` (`3e6645f`, done a prior session)
When menus emit cmTile/cmCascade/cmDosShell, the deferred bodies land
**together** — build
`Desktop::tile`/`cascade` geometry (`mostEqualDivisors`/`iSqr`/`calcTileRect`/
`dividerLoc`/`doCascade`, `tdesktop.cpp`) + wire the breadcrumb in
`program_handle_event` (after `group.handle_event`, beside the QUIT catch, calling
`desktop.tile/cascade(get_tile_rect())`) + test it with real tileable windows in
one change. `dosShell` separately needs a backend terminal-suspend seam + SIGTSTP.

### Phase 4 — the immediate next work, in PORT-ORDER order
**Menus 46/49/50/51/52 — the WHOLE menu modal layer (bar+box+popup, keyboard+mouse)
— DONE** (see the per-session sections above). The command-graying "Context
command-set query" was resolved for menus as the row-49 **`Deferred::UpdateMenu`
broker** (NOT a `Context` read-accessor — that earlier framing is obsolete; the
broker is the established pattern). Remaining Phase-4 work, in order:

- **Status line:** `TStatusItem`/`TStatusDef` (47) + `TStatusLine` draw/data slice
  (53) — **DONE** (see "What landed THIS session" above). Same broker pattern,
  cached on the view. Its interactive arms land with the Program wiring below.
- **Wire a real menu bar + status line into `Program`** — lets the
  `examples/hello.rs` demo grow a real menu bar + status line (and shifts the desktop
  down — revisit the `ModalFrame`/`DragCapture` "(0,0)-desktop absolute-coords"
  caveats then). First emitter of `cmTile`/`cmCascade`/`cmDosShell` → wire the row-32
  breadcrumb + build `Desktop::tile`/`cascade` geometry; close the carried
  initial-regray gap (initial `Deferred::UpdateMenu` on menu-bar insert).

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
  **Worktree location:** `isolation:"worktree"` now lands under
  `/scratch/oetiker/claude-worktrees/` via the `WorktreeCreate` hook — but only
  after a session **restart** (hooks load at startup). Before that, isolation goes
  to the project's `.claude/worktrees/`; create the worktree manually at the
  `/scratch` path + dispatch a **non-isolated** subagent instead (the row-32
  cadence). Verify where a probe worktree actually lands before relying on it.
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
- **`Context` command-set query** (command-graying) — TButton + TInputLine still
  wait on it (to enable/disable cmCut/Copy/Paste etc.). **Menus did NOT need it** —
  row 49 resolved menu graying with the `Deferred::UpdateMenu` broker instead (the
  read-accessor framing is obsolete). A button/inputline consumer would either reuse
  that broker shape or add a read accessor when it lands.
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
