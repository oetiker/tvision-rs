# rstv — session handover (forward-looking)

> What the **next** session needs: current state, what's next, and the
> non-obvious gotchas. The per-session implementation narrative + the git-commit
> changelog live in
> [`docs/IMPLEMENTATION-LOG.md`](file:///home/oetiker/checkouts/rstv/docs/IMPLEMENTATION-LOG.md).
> Read this, then [CLAUDE.md](file:///home/oetiker/checkouts/rstv/CLAUDE.md)
> (orientation / locked decisions / cross-cutting seams), then start.
>
> **Direction = [`docs/BACKLOG.md`](file:///home/oetiker/checkouts/rstv/docs/BACKLOG.md)**
> (the PORT-ORDER successor). All 92 PORT-ORDER rows are ✅ — the post-port
> **backlog run** is the work now: FOUNDATION seams (Phase A, done) →
> mechanical fan-out (Phase B, nearly done) → Phase C backlogged features.
> When a row lands: add an IMPLEMENTATION-LOG section, tick the BACKLOG row,
> update this file.

## Current state (2026-06-10, B5 committed)

**HEAD = `c917b4b`; 1127 lib tests green; clippy + fmt clean.**

### What is on `main` (committed):
- **B1 ✅ (`680aabc`)** — button `cmCommandSetChanged` graying; `Program::new` seeds `command_set_changed: true` for initial broadcast. InputLine `can_update_commands`/`update_commands` from `handle_event` + `set_state`.
- **B3 ✅ (`680aabc`)** — InputLine cmCut/cmCopy/cmPaste; `Deferred::InputLinePaste` broker; `paste_text` (save_state + max_len clamp + check_valid).
- **B6 ✅ (`6ae0222`)** — FileDialog/ChDirDialog `wfGrow`; screen-relative resize deferred to first `handle_event`; `SearchRec` attr/size/time from `std::fs` + `pack_dos_time`.
- **B5 ✅ (`c917b4b`)** — `View::on_bounds_changed` hook; `Scroller::on_bounds_changed`; `list_viewer::on_bounds_changed` free fn (resize formula) adopted by all 5 ListViewer concrete types; Outline uses Scroller formula; `Window::locate` re-pushes `set_zoomed`; `KeyboardResizeCapture` (full keyboard resize: arrows/Ctrl/Home/End/PgUp/PgDn/Enter/Esc); `cmResize` enabled when `sfSelected && (wfMove || wfGrow)`. Resolves TODO(33d).

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

## Next — the remaining backlog (small, all unblocked)

- **B8 — small singletons:** `max_len` clamp on `InputLine::set_value`
  (note: the new `paste_text` DOES clamp; only the `set_value` flowback is
  unclamped), `TODO(valid-select)` (likely unblocked — `valid` takes ctx),
  timer payload (`program.rs`), `application.rs` init/doneHistory, help-ctx
  propagation plumbing.
- **Phase C stays backlogged (user decision):** editor find/replace dialogs,
  right-click context menu, internal-clipboard editor, D10 dialog
  gather/scatter group-walk, cmQuit-veto / saveAs-modified-close inline
  drives, cmDosShell (needs a backend suspend seam + SIGTSTP), help-ctx
  `OneOf` status line, theme editor (needs the D7 extension point;
  `Program::color_dialog` is the ready entry point), C9 kbPaste/bracketed
  paste.

## Editor seam leftovers (still open, latent — unchanged this session)

- **cmQuit veto:** `valid_end`'s app-quit path vetoes close of a modified
  `FileEditor` without a prompt; fix = a whole-tree analogue of
  `validate_modal_close`. *(Cheap interim: gate `FileEditor::valid`'s prompt
  to `cmd == cmClose`.)*
- **saveAs modified-close path:** `valid()` vetoes the close, then the
  saveAs dialog opens separately (deferred fires next pump); full fix =
  `validate_modal_close` drives `OpenSaveAsDialog` inline.
- **`edReadError` on load** (ctor has no ctx) — breadcrumbed.
- **`FileEditor::saveAs` itself is DONE** (`Deferred::OpenSaveAsDialog` →
  `ModalCompletion::SaveAsPick`; accept test is `!= CANCEL` — FD_OK_BUTTON
  ends with `cmFileOpen`, not `cmOK`). The `widgets::editor_mut` hatch peels
  FileEditor/Memo to the inner `Editor` for the brokers.

## Non-obvious gotchas (read before starting)

- **Worktrees** live under `/scratch/oetiker/claude-worktrees/<project>-<name>`.
  Create manually (`git worktree add <path> -b <branch>`) and dispatch
  non-isolated subagents pointed at the path. **Give each parallel agent its
  own `CARGO_TARGET_DIR`** (e.g. `/home/oetiker/scratch/cargo-target-<tag>`)
  — a shared target dir makes their "clean" claims unreliable. ALWAYS
  re-verify on the integrated tree with the canonical
  `CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target`.
- **Run `git merge` in `/home/oetiker/checkouts/rstv`**, never inside a
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
  `tvision-macros/src/specs.rs` (the `delegate_view` spy test catches
  existing methods, not brand-new defaulted ones). A new `Deferred` variant
  needs NO forwarder. Validator-trait methods are NOT `View` methods.
