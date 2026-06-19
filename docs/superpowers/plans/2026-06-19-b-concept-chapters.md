# Workstream B — mdBook Concept Chapters Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Fill every Part-2 concept GAP, partial-GAP, and `→ concept` route from `docs/audit/concept-coverage.md` with mdBook narrative, so a developer can understand mechanisms that no single symbol carries (idle, coordinate translation, event phase, modal execution, …).

**Architecture:** One task per **target chapter file** (edits to one `.md` must serialize; distinct chapters are file-disjoint and parallelize). Each task adds one or more clearly-headed subsections, each explaining a named mechanism with a pointer to the source that implements it. Authoring is Rust-first (C++ in a `> **Turbo Vision heritage:**` blockquote); any new `rust` block follows the hidden-`# use tvision_rs as tv;` doctest convention.

**Tech Stack:** mdBook (`docs/book/`), `cargo xtask test` (per-chapter rustdoc doctest gate), `cargo xtask docs` (build + book↔api link check).

## Global Constraints

- `export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target`.
- Doc gates (integrated tree): `cargo xtask test` (guide doctests — prints real rustc errors), `cargo test --doc -p tvision-rs`, `cargo build --examples -j 2`, `cargo xtask docs` (build + link-check).
- **Doctest convention:** in the book the crate is `tvision-rs`, not `tv`. Every new ` ```rust ` block adds a hidden `# use tvision_rs as tv;` (or extern-free `# use tvision_rs::…;`); for calls on a live `Program`/`Context`/view, wrap in a hidden uncalled `# fn _demo(recv: &mut tv::Foo){…}`. Silence unused vars with a hidden `# #[allow(unused_variables)]`, never a visible `let _ =`.
- **`{{#rustdoc_include}}` blocks stay `rust,ignore`.** After any include edit, `grep -rl rustdoc_include docs/book/book` must be empty.
- Rust-first prose; C++ only in a `> **Turbo Vision heritage:**` blockquote. Do not add C++ framing to `apps/`/`getting-started/`/`internals/` primary prose.
- **Dependency on workstream C:** Task 2 below (event-loop: idle + getEvent) requires C's `Program::set_on_idle` (C plan Task 2) and C's getEvent doc-note (C plan Task 3) to be **landed first**. All other B tasks are independent of C.
- **Anchor contract for workstream A:** the six `→ concept` routes are served by stable subsection anchors named exactly: `#the-phase-field`, `#ending-a-modal-execview`, `#the-modal-loop-execute`, `#endmodal`, `#validator-error-dialogs`, `#draw-on-demand-vs-whole-tree`. Workstream A links to these; do not rename them after A starts.
- Commit messages end with: `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`.

## Authoring recipe (applies to every task)

For each item: (a) add a subsection with the specified heading to the named chapter; (b) explain the **mechanism** Rust-first — what the capability is, how a developer uses it, what the moving parts are; (c) cite the implementing source as `file_path` (and a one-line `> **Turbo Vision heritage:**` mapping where the C++ name aids recall); (d) if a code example clarifies, add a doctest-convention `rust` block; (e) run the doc gates. **Acceptance per item:** a reader who knew only the symbol names can now explain the mechanism from the chapter alone.

---

### Task 1: `internals/event-loop.md` — phase, clearEvent, cursor shape *(C-independent parts)*

**Files:** Modify `docs/book/src/internals/event-loop.md`.

Add these subsections (the idle + getEvent subsections are Task 2 — keep them separate so this task does not block on C):

- [ ] **The Phase field** — heading `## The Phase field` (anchor `#the-phase-field`). Explain why a view reads `ctx.phase()` and reacts differently per leg: an Alt-letter accelerator is handled in **pre-process**, a plain letter in **post-process**, focused keys in the **focused** leg. Source: `src/view/view.rs` (`enum Phase`), `src/view/context.rs` (`phase()`/`set_phase`), `src/view/group.rs` (router brackets each leg). Serves the `TGroup::Phase` `→ concept` route.
- [ ] **Block vs underline cursor** — heading `## Cursor shape: insert vs overwrite`. Explain `ViewState::cursor_ins` (block = overwrite, underline = insert) and how the loop places the hardware cursor. Source: `src/view/view.rs` (`cursor_ins`, `set_cursor`), `src/app/program.rs` (resetCursor walk).
- [ ] **Who handled the event (clearEvent)** — heading `## Marking an event handled`. Explain `Event::consume`/nothing-state and that "who handled it" is recorded by the handler returning/consuming, not a shared pointer. Source: `src/event/mod.rs`.

- [ ] **Gate + commit**

Run: `cargo xtask test && cargo xtask docs`
Expected: doctests pass, build + link-check clean.

```bash
git add docs/book/src/internals/event-loop.md
git commit -m "docs(guide): event-loop — phase field, cursor shape, clearEvent

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2: `internals/event-loop.md` — idle + getEvent *(REQUIRES workstream C landed)*

**Files:** Modify `docs/book/src/internals/event-loop.md`.

**Precondition:** C's `Program::set_on_idle` and the getEvent doc-note are on the integrated tree. Verify: `git grep -n "set_on_idle" src/app/program.rs` returns a hit before starting.

- [ ] **Idle-time / background work** — heading `## Background work each idle pass`. Explain the idle pass (`src/app/program.rs`, the `None =>` arm: command-set broadcast, timer expiry, status-line help-ctx) and how an app runs its own periodic work via `Program::set_on_idle` (clock/animation/heap-display). Show a doctest-convention example registering an idle hook. Note the alternative for exact timing: `Event::Timer` via the timer queue. `> **Turbo Vision heritage:**` successor to overriding `TProgram::idle`.
- [ ] **Injecting events / no getEvent override** — heading `## Where events come from`. Explain the single acquisition path (`Backend::poll_event` → internal queue → mouse-auto synth in `src/app/program.rs`), that there is deliberately no app-level `getEvent` override, and the idiomatic substitutes (timer queue, `set_on_idle`, headless event queue in tests). `> **Turbo Vision heritage:**` C++ `TProgram::getEvent`.

- [ ] **Gate + commit**

Run: `cargo xtask test && cargo xtask docs`

```bash
git add docs/book/src/internals/event-loop.md
git commit -m "docs(guide): event-loop — idle hook + event acquisition seam

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 3: `internals/custom-view.md` — coordinates, setState, custom colors

**Files:** Modify `docs/book/src/internals/custom-view.md` (and add cross-links from `internals/view-tree.md` / `apps/theming.md` where noted).

- [ ] **Local vs global coordinates** — heading `## Local and global coordinates`. Explain that a view draws in its own local space and the router subtracts each child's origin on the way down (no public `make_local`/`make_global`); show how a custom view converts a mouse position to local coords. Source: `src/view/group.rs` (`route_event`), `src/view/geometry.rs`.
- [ ] **Reacting to a state change** — heading `## Overriding set_state`. Explain overriding `View::set_state` to react (enable/disable, repaint) when a flag flips, calling the inner/default first. Source: `src/view/view.rs`, `src/view/group.rs`.
- [ ] **Giving a custom view its own colors** — heading `## A custom view's colors`. Explain choosing a `Role` and mapping it through the `Theme`; cross-link `apps/theming.md`. Source: `src/theme.rs`, `src/color.rs`.

- [ ] **Gate + commit**

Run: `cargo xtask test && cargo xtask docs`

```bash
git add docs/book/src/internals/custom-view.md docs/book/src/internals/view-tree.md docs/book/src/apps/theming.md
git commit -m "docs(guide): custom-view — coordinates, set_state, custom colors

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 4: `port/events.md` — abandoned events + event-mask narrative

**Files:** Modify `docs/book/src/port/events.md`.

- [ ] **Unhandled events** — heading `## When no one handles an event`. Explain that an unhandled event falls through the pump (no `eventError` abort); contrast with C++. Source: `src/app/program.rs`. `> **Turbo Vision heritage:**` C++ `eventError`.
- [ ] **Opting into expensive event classes** — heading `## Event masks`. Explain `EventMask` opt-in (mouse-move/auto are off unless a view asks) and the `wants`/`blocked` gate. Source: `src/event/mod.rs`, `src/view/group.rs`.

- [ ] **Gate + commit**

Run: `cargo xtask test && cargo xtask docs`

```bash
git add docs/book/src/port/events.md
git commit -m "docs(guide): events — abandoned-event path + event masks

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 5: `port/modal.md` — execView / endModal / Execute routes

**Files:** Modify `docs/book/src/port/modal.md`.

Serves three `→ concept` routes (`TGroup` ExecView, EndModal, Execute). Use the exact anchors from the Global Constraints.

- [ ] **Running a modal (execView)** — heading `## Ending a modal (execView)` (anchor `#ending-a-modal-execview`). Explain `exec_view` pushing a `ModalFrame` on the capture stack vs C++'s nested `Execute` loop. Source: `src/capture.rs`, `src/app/program.rs`.
- [ ] **The single modal loop (Execute)** — heading `## The modal loop (Execute)` (anchor `#the-modal-loop-execute`). Explain that tvision-rs has ONE loop in `Program` (`run` = `while end_state.is_none() { pump_once() }`); a modal is a capture frame, not its own loop. Source: `src/app/program.rs`.
- [ ] **Ending it (endModal)** — heading `## endModal` (anchor `#endmodal`). Explain `Deferred::EndModal` requesting modal termination with a result command. Source: `src/view/context.rs`, `src/app/program.rs`.

- [ ] **Gate + commit**

Run: `cargo xtask test && cargo xtask docs`

```bash
git add docs/book/src/port/modal.md
git commit -m "docs(guide): modal — execView/Execute/endModal concept routes

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 6: `port/draw.md` + `internals/drawing.md` — DrawView route + clip rect

**Files:** Modify `docs/book/src/port/draw.md` and `docs/book/src/internals/drawing.md`.

- [ ] **Draw-on-demand vs whole-tree** — in `port/draw.md`, heading `## Draw-on-demand vs whole-tree redraw` (anchor `#draw-on-demand-vs-whole-tree`). Explain that C++ `DrawView` drew only if `Exposed`; tvision-rs redraws the whole tree each pump and diffs against the prior buffer — there is no `draw_view`. Serves the `TView::DrawView` `→ concept` route. Source: `src/screen/`, `src/app/program.rs`.
- [ ] **Clipping** — in `internals/drawing.md`, heading `## Clipping to owner bounds`. Explain that cells are clipped at owner bounds in the buffer writer (the `getClipRect` successor) and that partial/clip-driven draw isn't a separate user step. Source: `src/screen/`.

- [ ] **Gate + commit**

Run: `cargo xtask test && cargo xtask docs`

```bash
git add docs/book/src/port/draw.md docs/book/src/internals/drawing.md
git commit -m "docs(guide): draw — DrawView route + clip-rect clipping

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 7: `apps/windows.md` — drag limits, grow anchors, Z-reorder

**Files:** Modify `docs/book/src/apps/windows.md`.

- [ ] **Drag limits** — heading `## Limiting move and resize`. Explain the `DragMode` limit bits (which edges/directions a drag may change). Source: `src/view/view.rs`, `src/window/window.rs`.
- [ ] **Grow modes** — heading `## Grow modes: anchoring edges`. Explain the anchor-edges model (`GrowMode` ties a view's edges to its owner on resize) head-on. Source: `src/view/view.rs`, applied on `change_bounds`.
- [ ] **Restacking** — heading `## Bringing a window to the front`. Explain Z = reverse insertion order and raise-on-select; note there's no general "reorder arbitrary view in Z" primitive beyond raise-to-top. Source: `src/view/group.rs`.

- [ ] **Gate + commit**

Run: `cargo xtask test && cargo xtask docs`

```bash
git add docs/book/src/apps/windows.md
git commit -m "docs(guide): windows — drag limits, grow anchors, restacking

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 8: `apps/controls.md` + `apps/dialogs.md` — transfer order, validation, history

**Files:** Modify `docs/book/src/apps/controls.md` and `docs/book/src/apps/dialogs.md`.

- [ ] **Tab order = transfer order** — in `apps/dialogs.md`, heading `## Tab order and data transfer`. State the tie: insertion/Z-order drives both Tab navigation and gather/scatter order. Source: `src/view/group.rs`.
- [ ] **Validate on focus change** — in `apps/controls.md`, heading `## Validating a field`. Explain `ofValidate` holding focus until the field is valid. Source: `src/view/group.rs`.
- [ ] **Validate on demand & on close** — same chapter, heading `## Validating without closing`. Explain `valid(cmClose)` group-walk used on-demand and at modal close. Source: `src/view/group.rs`, `src/dialog/dialog.rs`.
- [ ] **Validator error dialogs** — heading `## When validation fails` (anchor `#validator-error-dialogs`). Explain `Validator::error` requesting a message box via the async-modal seam. Serves the `TStringLookupValidator::Error` `→ concept` route. Source: `src/validate.rs`, `src/view/context.rs`.
- [ ] **History persistence** — in `apps/controls.md`, heading `## History lists`. Explain recall by `history_id` channel and the store/load idiom. Source: `src/widgets/history.rs`.
- [ ] **Change-directory dialog** — in `apps/dialogs.md`, expand the change-dir dialog coverage. Source: `src/dialog/`.

- [ ] **Gate + commit**

Run: `cargo xtask test && cargo xtask docs`

```bash
git add docs/book/src/apps/controls.md docs/book/src/apps/dialogs.md
git commit -m "docs(guide): controls/dialogs — transfer order, validation, history

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 9: `apps/text-editing.md` — editor commands, bindings, find/replace, memo

**Files:** Modify `docs/book/src/apps/text-editing.md`.

- [ ] **Editor command self-gating** — heading `## Edit commands enable themselves`. Explain how the editor enables/disables cut/copy/paste by state. Source: `src/widgets/editor.rs`.
- [ ] **Key bindings** — heading `## Key bindings`. Enumerate the WordStar/Ctrl-K block bindings (and the configurable keymap presets). Source: `src/widgets/editor.rs`, `src/keymap.rs`.
- [ ] **Find and replace** — heading `## Search and replace`. Document the find/replace flow (dialogs, options, prompt-on-replace). Source: `src/widgets/editor.rs`.
- [ ] **Memo as a control** — heading `## Editor as a dialog control (Memo)`. Explain the Memo wiring (traps Tab, getData/setData via the value protocol). Source: `src/widgets/editor.rs`, `src/data.rs`.

- [ ] **Gate + commit**

Run: `cargo xtask test && cargo xtask docs`

```bash
git add docs/book/src/apps/text-editing.md
git commit -m "docs(guide): text-editing — command gating, bindings, find/replace, memo

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 10: `apps/menus.md` + `internals/brokering.md` + gallery — hints, messaging, outline/terminal

**Files:** Modify `docs/book/src/apps/menus.md`, `docs/book/src/internals/brokering.md`, `docs/book/src/gallery.md`.

- [ ] **Context hint text** — in `apps/menus.md`, heading `## Context-sensitive hints`. Draw out hint-by-context override in the status line. Source: `src/status/`.
- [ ] **Inter-view messaging** — in `internals/brokering.md`, heading `## Broadcast as a message`. Explain broadcast-as-message and the "find topmost-of-type / who-handled" probe idiom. Source: `src/view/context.rs`, `src/view/group.rs`.
- [ ] **Outline & terminal in context** — in `gallery.md` (or the nearest apps chapter), ensure `Outline` and `Terminal` each have a short usage paragraph, not just a screenshot. Source: `src/widgets/outline.rs`, `src/widgets/terminal.rs`.

- [ ] **Gate + commit**

Run: `cargo xtask test && cargo xtask docs`

```bash
git add docs/book/src/apps/menus.md docs/book/src/internals/brokering.md docs/book/src/gallery.md
git commit -m "docs(guide): menus/brokering/gallery — hints, messaging, outline+terminal

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Self-Review

- **Spec coverage:** Every hard GAP (coordinate translation T3, cursor shape T1, eventError T4, getEvent T2, idle T2), all partial-GAPs (T1,3,4,7,8,9,10), and all six `→ concept` routes (Phase T1; ExecView/Execute/EndModal T5; Error T8; DrawView T6) map to a task. The `concept-coverage.md` rows are exhausted.
- **Placeholder scan:** Each item names the chapter, the subsection heading, the mechanism to explain, and the source file. Prose itself is the implementer's deliverable (appropriate for doc authoring); the teaching point and source anchor are concrete, so no item is vague.
- **Anchor consistency:** The six `→ concept` anchors are declared once in Global Constraints and reused verbatim in Tasks 1/5/6/8 — and A links to those same strings. No rename after A starts.
- **Dependency:** Only Task 2 depends on C (verified by a `git grep set_on_idle` precondition); Tasks 1,3–10 are C-independent and fully parallel with each other and with A.
