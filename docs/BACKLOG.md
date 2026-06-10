# rstv — post-port backlog (the PORT-ORDER successor)

> All 92 PORT-ORDER rows are ✅. This doc orders the **remaining TODOs** the
> port deliberately deferred (audited 2026-06-10 against every `TODO` marker in
> the tree + HANDOVER's standing deferrals). Same contract as PORT-ORDER:
> **the lowest unmarked row in the active phase is the work**, ✅ = done & on
> `main`, tags pick the implementer model (FOUNDATION → Opus/main thread,
> MECHANICAL → Sonnet). When a row lands: tick it here, add the
> IMPLEMENTATION-LOG entry, update HANDOVER.
>
> **The architecture rule for this run:** most open TODOs cluster behind a few
> missing seams. Phase A builds those seams *once*, with a design note in
> `docs/design/` and a PORTING-GUIDE D-rule where behavior deviates from the
> C++; Phase B then adopts them mechanically per widget. Never hand-roll a
> per-widget variant of a Phase-A seam.
>
> **Run-scope decisions (user, 2026-06-10):** the big editor sub-features are
> Phase C — *backlogged, not in this run*. The theme verification pass (A4)
> runs **early, in parallel** with A1–A3.

## Phase A — FOUNDATION seams (serial, except A4 ∥)

| row | item | tag | unblocks | notes |
|---|---|---|---|---|
| A1 ✅ | **CommandSet allowlist → denylist flip** | FOUNDATION 🔴 | B1 | `src/command.rs` `CommandSet` is an allowlist (default-empty → everything disabled); C++ `tview.cpp::initCommands` is **enabled-by-default** with a 5-command denylist (cmZoom/cmClose/cmResize/cmNext/cmPrev) + every command `>255` always enabled. Flip `has` to `!disabled.contains`, seed startup with the 5 window commands disabled, shrink `program.rs::default_command_set` accordingly (the file-dialog bandaid entries disappear). Add the **`Context` command-set query** + `cmCommandSetChanged` broadcast plumbing (C++ `commandSetChanged` flag) so views can gray themselves. |
| A2 ✅ | **`show()→resetCurrent` cascade at insert** | FOUNDATION 🔴 | — | C++ `TView::setState(sfVisible)` calls `owner->resetCurrent()` for `ofSelectable`. rstv's ctx-less `Group::insert` skips it; compensated in **three** call sites (`exec_view`, `HistoryWindow::select_child`, `Desktop::insert_and_focus`). Establish currency at insert/show time and collapse the compensations. See memory `show-resetcurrent-cascade-gap`; partial fix landed e8d82f2. |
| A3 ✅ | **D9 press-and-hold / auto-repeat capture seam** | FOUNDATION | B2 | Generalize `window.rs::DragCapture` + the button hold-capture (9aa291d) into one reusable capture: track-while-held, fire-on-release-inside, optional timer-driven auto-repeat (scrollbar arrows). Design note required; this is the C++ `while(mouseEvent(...))` inner-loop successor. |
| A4 ✅ | **Theme chain verification pass** (∥ with A1–A3) | FOUNDATION | B-theme leaves | Per memory `theme-trust-the-chain`: every "provisional" `theme.rs` value gets derived from the **literal C++ palette chain** and the chain documented inline (menu :637, status-line :651, label :604, input :615, cluster/static :569, indicator :577). Plus the **cyan window scheme** (`frame.rs:40,175,223`, `window/window.rs:46,53,256` — blue fallback today), the window-scheme list remap (`theme.rs:552`), and the history palette remaps (`history.rs:180,363,547`). Snapshot churn expected — review every changed `.snap` against the chain. |
| A5 ✅ | **Phased key dispatch (preProcess/focused/postProcess)** | FOUNDATION | B4 | C++ `TGroup::handleEvent` routes `evKeyDown` through phases; rstv has no phase concept, blocking every "plain-hotkey postProcess accelerator" TODO (`static_text.rs:522`, `button.rs:445`, `cluster.rs:533`). Small seam; design note required. |
| A6 ✅ | **OS clipboard integration** | FOUNDATION | B3, C9 | **User directive (2026-06-10): rstv integrates with the OS clipboard by default — the internal string buffer is a last-resort fallback only, never the default.** Backend `Clipboard` seam: OSC 52 write (works over SSH; `crossterm_backend.rs:159`) and/or a native provider (e.g. `arboard`), with capability detection choosing the best available; paste arrives via bracketed paste (B7's event). magiblot tvision integrates with the system clipboard — this is also the *faithful* choice. Design note required (provider selection + fallback order). |

## Phase B — MECHANICAL fan-out (parallel worktree batches once their seam is ✅)

| row | item | needs | sites |
|---|---|---|---|
| B1 ✅ | command-graying adoptions | A1 | **COMPLETE (`680aabc`):** button `cmCommandSetChanged` arm sets `state.disabled = !ctx.command_enabled(command)`; initial graying on startup via `command_set_changed: true` in `Program::new`. InputLine `can_update_commands`/`update_commands` pushed from `handle_event` tail + `set_state` transition. |
| B2 ✅ | press-and-hold adoptions | A3 | **COMPLETE (`90fc0ce`,`de1c0f0`,`62bbd15`,`eb7648d`,`694963d`):** all 8 — scrollbar, inputline, cluster, frame, listviewer, outline, statusline (+ first-class pre-route deferreds in the pump), editor (+ the untracked-wheel fidelity fix) |
| B3 ✅ | InputLine clipboard | A6 | **COMPLETE (`680aabc`):** cmCut/cmCopy write to backend clipboard via `ctx.set_clipboard`; cmPaste uses `Deferred::InputLinePaste` broker (pump reads clipboard, calls `paste_text`). `paste_text` bulk-inserts with `save_state`/`check_valid`/`max_len` clamp + scroll-follow + `sync_cursor`. |
| B4 ✅ | accelerator adoptions + ctrlToArrow | A5 | **landed WITH A5** (`43c9d30`): button/label plain-letter postProcess accelerators, cluster accelerator scan, ctrl_to_arrow for cluster + scrollbar |
| B5 ✅ | resize republish family | — | **COMPLETE (`c917b4b`):** `View::on_bounds_changed` hook (pump post-ChangeBounds); `Scroller::on_bounds_changed` (set_limit); `list_viewer::on_bounds_changed` free fn (vbar pgStep=size.y, hbar pgStep=size.x/numCols) adopted by ListBox/SortedListBox/HistoryViewer/FileList/DirListBox; Outline uses Scroller formula; `Window::locate` re-pushes `frame.set_zoomed`; `KeyboardResizeCapture` (arrows/Ctrl/Home/End/PgUp/PgDn/Enter/Esc); cmResize enabled in `set_state`. |
| B6 ✅ | FileDialog finishers | — | **COMPLETE (`6ae0222`):** `wfGrow` for FileDialog + ChDirDialog via `Dialog::set_flags`; "21st-century percentages" screen-relative resize deferred to first `handle_event` (`needs_screen_resize` guard, `ctx.owner_size()`); `SearchRec` attr/size/time populated from `std::fs` metadata (`FA_DIREC` for dirs, `pack_dos_time` DOS ftime packing). |
| B7 ✅ | backend terminal lifecycle | — | **User confirm (2026-06-10): a TV program must not hand-roll terminal setup — C++ `TApplication`/`TScreen` does it in the ctor.** Move raw-mode/alt-screen/mouse-capture from `examples/hello.rs:218` into `CrosstermBackend` (RAII: setup at construction, teardown in `Drop` + a panic hook so a crashed app restores the terminal) and strip it from `hello.rs`. Also: paste event (`crossterm_backend.rs:313`), focus events (`:317`). Can run early — independent of Phase A. |
| B8 | small singletons | — | `input_line.rs:744` (`max_len` clamp on `set_value`), `input_line.rs:702` (`valid-select` — likely unblocked: `valid` now takes `ctx`), `program.rs:1375` (timer payload), `application.rs:47,63` (init/doneHistory), `program.rs:1776` (help-ctx propagation plumbing) |

## Phase C — backlogged feature work (NOT this run; own run each)

| row | item | notes |
|---|---|---|
| C1 | Find/Replace dialogs | `editorDialog` + std dialog views; `editor.rs:1452,1655`; `search()` itself is live |
| C2 | Editor right-click context menu | `initContextMenu` + `popupMenu` machinery; `editor.rs:1585` |
| C3 | Internal-clipboard editor | `insertFrom` branch + clipboard `EditWindow`; `editor.rs:1418,1440`; `EditWindow::close` hide-branch breadcrumb |
| C4 | D10 dialog gather/scatter group-walk | `list_box.rs:30,159`; deferred to its first multi-field consumer |
| C5 | cmQuit veto / saveAs modified-close inline drives | the whole-tree `validate_modal_close` analogue (HANDOVER "Editor seam leftovers") |
| C6 | `cmDosShell` | backend terminal-suspend seam + SIGTSTP |
| C7 | help-ctx refresh / `OneOf` status line | needs `View::get_help_ctx` + TopView resolver |
| C8 | Theme editor | consumes `color_dialog`; needs the D7 Theme extension point first |
| C9 | kbPaste / bracketed-paste multi-char insert | `editor.rs:1628` + backend paste event (B7 lands the event; this is the editor consumer) |

## Latent edge notes (keep as TODOs; fix opportunistically)
`input_line.rs:334` auto-fill shrink clamp (D13 hazard, no auto-fill validator
exists yet) · `editor.rs:899` setBufSize shrink · `:910` OOM path (Vec
infallible) · `:1766` charScan.scanCode · `:2125` fexpand nuance · `:2170`
efBackupFiles · `edReadError` on load (ctor has no ctx) ·
`menu_session.rs:1159` TMenuPopup Ctrl+letter (dead under the capture-stack
model, documented).
