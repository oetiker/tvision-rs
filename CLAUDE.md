# rstv — idiomatic Rust port of Turbo Vision

**What this is:** a faithful Rust port of **magiblot/tvision** (modern C++ Turbo
Vision). The goal is a framework a C++ tvision veteran recognizes on sight, but
that is native Rust.

## Read these first
- **`docs/HANDOVER.md`** — the **forward-looking handover**: where things stand
  (HEAD commit, build state), what's next, and the non-obvious gotchas the next
  session needs. Read it after this file. **It is intentionally slim** — the
  per-session implementation narrative + git-commit changelog live in
  `docs/IMPLEMENTATION-LOG.md` (newest first; append, don't rewrite).
- **`docs/IMPLEMENTATION-LOG.md`** — the **changelog**: what each session built
  and why, per commit. History, not orientation — consult it for the rationale
  behind a landed piece; do not read it cover-to-cover to start.
- **`docs/PORT-ORDER.md`** — dependency-ordered checklist of 92 classes in 6
  phases, with verified C++ file mappings, target Rust modules, and
  `FOUNDATION`/`MECHANICAL`/`INFRA` tags (**✅ = done & on `main`**). **Port in
  this order** (lowest-numbered unmarked row is the work; "Parallelizable
  batches" are an efficiency, not a
  competing direction).
- **`docs/PORTING-GUIDE.md`** — the deviation reference. We port *faithfully*
  from the C++; this guide documents **only the places we deviate** (D1–D13),
  each as *Baseline → Deviation → Integration*. Appendix A = C++→Rust symbol
  lookup. Appendix B = the **mechanical per-class porting recipe**.
- **`docs/design/`** — design notes for cross-cutting seams (e.g.
  `deferred-effects.md`, `delegation-macros.md`).

## Source trees (not in this repo)
- **Port FROM:** `/home/oetiker/scratch/tvision-spec/magiblot-tvision/`
  (headers `include/tvision/`, impl `source/tvision/`, platform
  `source/platform/`). This is the source of truth — port its behavior verbatim.
- **Lessons reference only:** `/home/oetiker/scratch/tvision-spec/tvision/` is a
  working **Go** port. It was already mined for lessons. **Never reference the Go
  port in the guide or commits** — the guide is purely C++→Rust.

## Commands
This is a **Cargo workspace** (`rstv` + `rstv-macros`) — use `--workspace`.

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target  # artifacts land HERE, not ./target
cargo test  --workspace
cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
cargo run --example hello   # first runnable TV app: modal dialog on a desktop
```

Verification is **snapshot tests** (D11, `insta`): build a view on the
`HeadlessBackend`, `render`, `assert_snapshot!` against the frozen format in
`src/screen/snapshot.rs`. Every widget gets one.

## Methodology (lean by design)
1. **Faithful by default.** If a class/behavior isn't called out as a deviation,
   translate it straight from the C++. No per-file design.
2. **Deviations are pre-decided** in the guide. Apply the relevant D-rules
   mechanically (Appendix B has the line-level substitution table).
3. **Division of labor:** `INFRA` (net-new substrate) and `FOUNDATION`
   (pattern-setting classes) need careful Opus/human work. `MECHANICAL` leaves
   are handed to **Sonnet** via Appendix B + the PORT-ORDER row — they need
   near-zero judgment. Parallelizable batches are listed in PORT-ORDER.md.
4. **Snapshot tests are the verification** (D11): port a piece, run it on the
   `HeadlessBackend`, snapshot, compare to C++ behavior. No heavy upfront plans.

## How to run the port (subagent-driven, the default from Phase 1 on)

Phase 0 was FOUNDATION/INFRA — interlocking, design-heavy, mostly serial. Phase 1+
is **mostly `MECHANICAL` leaf widgets in parallel batches** (PORT-ORDER Batches
A–E), so the orchestrator runs it as **subagent-driven development**
(`superpowers:subagent-driven-development`). The main thread does **only**
coordination: design FOUNDATION seams, write precise prompts, integrate, decide.
Per row:

1. **Implementer subagent (fresh, isolated context).** Give it a *self-contained*
   prompt — never "go read the plan." Inline: the PORT-ORDER row, the relevant
   C++ source (from `magiblot-tvision/`), the D-rules that apply (Appendix B
   table), the existing types it builds on, and "run `cargo test`/`clippy
   --all-targets`/`fmt --check` + add a snapshot test (Appendix B step 4)."
   **Model by tag:** `MECHANICAL` → Sonnet; `FOUNDATION`/`INFRA` → Opus (or the
   main thread).
2. **Two-stage review (fresh subagents — do NOT just self-review in the main
   thread).** First a **spec-compliance** reviewer (does it match the C++
   behavior + the row's D-rules, nothing extra/missing?), then, once that's ✅, a
   **code-quality** reviewer. Implementer fixes; re-review until clean.
   (`feature-dev:code-reviewer` / `gsd-code-reviewer` agent types fit, or a
   plain agent with the row's spec.)
3. **Integrate + verify in the shared tree**, then mark the row done.

**Parallelism (the reconciliation):** the skill says "never dispatch parallel
implementers" because of shared-tree conflicts — but PORT-ORDER's batches are
*build-disjoint*, so dispatch them **concurrently using `isolation: "worktree"`**
(each agent self-verifies in its own checkout; the orchestrator integrates). Run
serially only for shared files (`lib.rs`, a shared `mod.rs`) and FOUNDATION rows
that gate others. The orchestrator owns the few shared-file edits (module wiring,
re-exports) to avoid races.

**Worktree gotcha:** an agent worktree is branched from the last **commit**, so
uncommitted work is absent from it. **Commit completed rows before dispatching
worktree subagents that build on them.** Commit at batch boundaries. Worktrees
live under `/scratch/oetiker/claude-worktrees/<project>-<name>` (a
`WorktreeCreate` hook redirects `isolation:"worktree"` there; it activates only
after a session restart — until then, create the worktree manually at the
`/scratch` path and dispatch a non-isolated subagent).

## Locked decisions (details in the guide)
Crate `rstv`, house style `tv::`; drop `T` prefix; `snake_case` methods;
constant families → open newtypes with SCREAMING_SNAKE assoc consts
(`tv::Command::OK`); inheritance → `View` trait + `ViewState` composition;
pointers → `ViewId` handles + downward `Context`; events → `enum Event` + match;
flag words → struct-of-bools; palette+glyphs → `Theme`; whole-tree redraw + diff
(no damage tracking); modal loops → single loop + capture stack; `TStreamable`
dropped (serde if revived). Stack: crossterm (behind a `Backend` trait) →
vendored ratatui cell-buffer+diff (MIT) → retained view tree + event loop.

**Newtype vs enum by *extensibility*:** open/app-extensible families (`Command`,
`HelpCtx`) → open newtype with namespaced `&'static str` identity; closed sets
(`Key`) → enum. Constants live with their owner (no central registry).

Coordinates are `i32` (faithful to magiblot's `int`).

## Cross-cutting seams (the non-obvious substrate)
These underlie every widget; a new session needs them. Detail is in the cited
D-rule / design note, not here.

- **Single event loop** in `app::Program` (D9): `pump_once` =
  `getEvent`→capture-stack→`program_handle_event`→apply deferred→resetCursor→
  whole-tree redraw+diff. `run` is the outer `while !valid(end_state)`.
- **`Deferred` channel** (`docs/design/deferred-effects.md`): effects on
  loop-owned state that a downward-borrowed `&mut View` can't perform inline
  (push capture, enable/disable command, change bounds, set state, close, end
  modal, focus-by-id, scroller/list sync, scrollbar params, set-visible). **A new
  deferred capability ADDS A VARIANT**, not a `Context::new` param.
- **Cross-view sibling broker** (D3): a leaf view holds only `&mut Context`, so
  the **pump** brokers every scroller↔scrollbar / listviewer↔scrollbar read &
  write at deferred-apply (`group.find_mut(id)` + `as_any_mut`/`View::value()`).
  Established by row 27 `TScroller`, reused by row 28 `TListViewer`.
- **Global `ViewId`** (D3): one process-global monotonic `NonZeroU64`; each view
  knows its own id; resolve via `View::find_mut(id)` / `remove_descendant`.
- **`Event::Broadcast { command, source: Option<ViewId> }`** (D4): `source` is
  the resolvable C++ `infoPtr` subject — used as a filter, not a value carrier.
- **D10 value protocol** (`src/data.rs`): `FieldValue` typed currency +
  `View::value`/`set_value` (getData/setData successors); dialog gather/scatter
  group-walk deferred to its first consumer.
- **D2 embed-and-delegate** via the `#[delegate(to = field)]` macro — see
  Conventions below.

## Current state
**The live HEAD, build state, and what's next are in `docs/HANDOVER.md`;
completion is tracked per row (✅) in `docs/PORT-ORDER.md`; per-commit rationale
is in `docs/IMPLEMENTATION-LOG.md`.** This section deliberately does **not**
restate the phase-by-phase snapshot (it goes stale) — read those three. The
`#[delegate]` proc-macro (`rstv-macros`) is landed and adopted codebase-wide.

## Conventions
- English for all code/comments/identifiers (user-facing strings may be localized).
- Commit messages end with the project's Co-Authored-By trailer
- **Delegation (D2 embed-and-delegate):** a type that embeds an inner view forwards
  the un-overridden `View` methods via `#[delegate(to = <field>)]` (proc-macro in
  `rstv-macros`; re-exported as `rstv::delegate`). Write only the methods that
  differ; `skip(<m>)` leaves a method at its trait default. **When you add a `View`
  trait method, add a matching forwarder to `rstv-macros/src/specs.rs`** — the
  spy test `tests/delegate_view.rs` catches a forgotten forwarder for existing
  methods, but a brand-new defaulted method would silently not forward. Adopting
  the macro at an existing site is behaviour-preserving (`skip(...)` = exactly what
  the site leaves defaulted, verified by a `cargo expand` method-set diff). Full
  rationale: `docs/design/delegation-macros.md`.
- **Keep this file stable.** Per-row progress goes in
  `docs/IMPLEMENTATION-LOG.md` (the changelog), not here. `HANDOVER.md` is the
  slim forward-looking state; CLAUDE.md is orientation.
