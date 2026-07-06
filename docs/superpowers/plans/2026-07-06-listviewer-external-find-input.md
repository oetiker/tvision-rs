# `ListViewer` external find-query input — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add a host-callable `ListViewer::set_find_query(query, ctx)` so a source outside the list's own keystroke handler (e.g. an `InputLine` above it in a combobox) can drive the list's incremental find, and fold `clear_find` into it so a single mutator carries the change tail.

**Architecture:** Purely additive on the shared `list_viewer` trait. Extract the existing broadcast+hook tail of `find_after_change` into a private `find_notify(this, ctx)` free fn; add `set_find_query` as a trait default method that guards, replaces `lv.query`, and calls `find_notify`; re-express the existing `clear_find` as a one-liner onto `set_find_query("")`. No concrete widget, `draw`, `find_route_key`, `filtered_view`, or `LIST_FIND_CHANGED` changes.

**Tech Stack:** Rust (workspace `tvision-rs`), `insta`/unit tests on the `HeadlessBackend`/headless `Context`, cargo test/clippy/fmt gates.

## Global Constraints

- Artifacts land in `/home/oetiker/scratch/cargo-target` — `export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target` before any cargo command.
- Cargo workspace — run gates with `--workspace`. Never use more than 4 cores: append `-j4` to build/test where it spawns compilation.
- English for all code/comments/identifiers.
- `git pull` (ff-only) before starting; roll `CHANGELOG.md` `## Unreleased` for the user-visible change.
- Commit messages end with the project's `Co-Authored-By` trailer.
- **Faithful-port note:** this is an rstv-*original* extension (spec §Type), not a C++ port — no `magiblot-tvision/` source to match; the spec is the source of truth.
- **Not a `View` trait method:** `set_find_query`/`clear_find` live on the `ListViewer` trait, so the `#[delegate]` macro / `tvision-rs-macros/src/specs.rs` forwarder list is **unaffected** — do not touch it.
- The three gates that must pass before every commit:
  ```bash
  export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
  cargo test  --workspace -j4
  cargo clippy --workspace --all-targets -j4 -- -D warnings
  cargo fmt --all --check
  ```

**Spec:** `docs/superpowers/specs/2026-07-06-listviewer-external-find-input-design.md`

---

### Task 1: Extract the shared change tail into `find_notify`

Pure refactor — no behaviour change. `find_after_change` currently inlines the broadcast + `on_query_changed` before `ev.clear()`. Pull those three lines into a private `find_notify(this, ctx)` free fn so the host-callable path (Task 2) can reuse them without an event to consume. Existing tests must stay green unchanged.

**Files:**
- Modify: `src/widgets/list_viewer.rs:661-668` (the `find_after_change` free fn)

**Interfaces:**
- Consumes: existing `ListViewer::lv() -> &ListViewerState`, `ViewState::id() -> Option<ViewId>`, `Context::broadcast(Command, Option<ViewId>)`, `ListViewer::on_query_changed(&mut Context)`.
- Produces: `fn find_notify<L: ListViewer + ?Sized>(this: &mut L, ctx: &mut Context)` — private module free fn; broadcasts `Command::LIST_FIND_CHANGED` with `this.lv().state.id()` as source, then calls `this.on_query_changed(ctx)`. Does **not** touch the event. Task 2's trait methods call it.

- [ ] **Step 1: Run the existing find tests to establish the green baseline**

Run:
```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test --workspace -j4 find
```
Expected: PASS (includes `find_mode_accumulates_query_and_broadcasts_on_change`, `find_backspace_and_esc_behaviour`, `list_box_self_filter_narrows_and_restores`, …).

- [ ] **Step 2: Replace `find_after_change` with `find_notify` + a thin wrapper**

In `src/widgets/list_viewer.rs`, replace the current free fn (lines 661-668):

```rust
/// Common tail after the query changes: broadcast the change (self as `source`,
/// mirroring `select_item` / `ScrollBar`), run the self-filter hook, consume.
fn find_after_change<L: ListViewer + ?Sized>(this: &mut L, ev: &mut Event, ctx: &mut Context) {
    let source = this.lv().state.id();
    ctx.broadcast(Command::LIST_FIND_CHANGED, source);
    this.on_query_changed(ctx);
    ev.clear();
}
```

with:

```rust
/// The shared find-query change tail: broadcast the change (this list as
/// `source`, mirroring `select_item` / `ScrollBar`) and run the self-filter hook.
/// Reused by the keystroke path (`find_after_change`, which also consumes the
/// event) and the host-callable path (`set_find_query` / `clear_find`, which have
/// no event to consume).
fn find_notify<L: ListViewer + ?Sized>(this: &mut L, ctx: &mut Context) {
    let source = this.lv().state.id();
    ctx.broadcast(Command::LIST_FIND_CHANGED, source);
    this.on_query_changed(ctx);
}

/// Keystroke-path tail: the shared `find_notify` plus consuming the key event.
fn find_after_change<L: ListViewer + ?Sized>(this: &mut L, ev: &mut Event, ctx: &mut Context) {
    find_notify(this, ctx);
    ev.clear();
}
```

- [ ] **Step 3: Verify the refactor is behaviour-preserving**

Run:
```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test --workspace -j4 find
cargo clippy --workspace --all-targets -j4 -- -D warnings
cargo fmt --all --check
```
Expected: all PASS, no clippy warnings, fmt clean. (No test text changed — this proves the extraction preserved the keystroke path.)

- [ ] **Step 4: Commit**

```bash
git add src/widgets/list_viewer.rs
git commit -m "ListViewer: extract shared find_notify tail from find_after_change

Pure refactor: the broadcast + on_query_changed tail is pulled out of
find_after_change so a host-callable query mutator can reuse it without an
event to consume. find_after_change now = find_notify + ev.clear(). No
behaviour change; existing find tests unchanged.

Co-Authored-By: <project trailer>"
```
(Use the repo's actual Co-Authored-By trailer from a recent commit; do not invent one.)

---

### Task 2: Add `set_find_query` and fold `clear_find` into it

Add the host-callable mutator on the `ListViewer` trait, re-express `clear_find` as a delegation to `set_find_query("")`, document both, and cover the shared-core semantics with unit tests on the existing `FakeList` harness. Roll the CHANGELOG — this is the user-visible deliverable.

**Files:**
- Modify: `src/widgets/list_viewer.rs:369-380` (rewrite `clear_find`, add `set_find_query` after it)
- Modify: `src/widgets/list_viewer.rs` test module (add tests after `find_query_reflects_mode_and_emptiness`, currently ending line 2735)
- Modify: `CHANGELOG.md:13` (under `## Unreleased` → `### New`)

**Interfaces:**
- Consumes: `find_notify(&mut L, &mut Context)` from Task 1; `ListViewer::lv()`, `lv_mut()`, `ListViewerState { find_mode: FindMode, query: String, state: ViewState }`, `FindMode::Off`.
- Produces:
  - `fn ListViewer::set_find_query(&mut self, query: &str, ctx: &mut Context)` — trait default method. No-op when `lv().find_mode == FindMode::Off` **or** `lv().query == query`; else sets `lv_mut().query = query.to_string()` and calls `find_notify(self, ctx)`.
  - `fn ListViewer::clear_find(&mut self, ctx: &mut Context)` — now defined as `self.set_find_query("", ctx)` (signature unchanged, behaviour preserved).

- [ ] **Step 1: Write the failing tests**

In the `src/widgets/list_viewer.rs` test module, add these tests immediately after `find_query_reflects_mode_and_emptiness` (after line 2735). They model the existing `find_mode_*` tests' use of `FakeList`, `make_ctx`, and broadcast counting.

```rust
    // -- set_find_query (external find input) ---------------------------------

    /// Count LIST_FIND_CHANGED broadcasts in an out-queue, and return the last
    /// broadcast's `source` if any.
    fn find_broadcasts(out: &VecDeque<Event>) -> (usize, Option<ViewId>) {
        let mut count = 0;
        let mut last_source = None;
        for e in out.iter() {
            if let Event::Broadcast { command, source } = e {
                if *command == Command::LIST_FIND_CHANGED {
                    count += 1;
                    last_source = *source;
                }
            }
        }
        (count, last_source)
    }

    #[test]
    fn set_find_query_sets_query_and_broadcasts_once() {
        let mut fake = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(3), None, None);
        fake.lv.find_mode = FindMode::Highlight;
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            fake.set_find_query("ab", &mut ctx);
        }
        assert_eq!(fake.find_query(), Some("ab"), "query is set");
        let (count, source) = find_broadcasts(&out);
        assert_eq!(count, 1, "exactly one LIST_FIND_CHANGED");
        assert_eq!(source, fake.state().id(), "source = this list's id");
    }

    #[test]
    fn set_find_query_same_text_is_a_noop() {
        let mut fake = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(3), None, None);
        fake.lv.find_mode = FindMode::Highlight;
        fake.lv.query = "ab".into();
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            fake.set_find_query("ab", &mut ctx);
        }
        assert_eq!(fake.lv.query, "ab", "unchanged");
        assert_eq!(find_broadcasts(&out).0, 0, "no broadcast on the change guard");
    }

    #[test]
    fn set_find_query_empty_clears_like_clear_find() {
        let mut fake = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(3), None, None);
        fake.lv.find_mode = FindMode::Highlight;
        fake.lv.query = "ab".into();
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            fake.set_find_query("", &mut ctx);
        }
        assert_eq!(fake.find_query(), None, "empty query reads as None");
        assert_eq!(fake.lv.query, "", "query emptied");
        assert_eq!(find_broadcasts(&out).0, 1, "empties and notifies once");
    }

    #[test]
    fn set_find_query_off_mode_is_total_noop() {
        let mut fake = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(3), None, None);
        // find_mode defaults to Off.
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            fake.set_find_query("x", &mut ctx);
        }
        assert_eq!(fake.lv.query, "", "Off: query untouched");
        assert_eq!(find_broadcasts(&out).0, 0, "Off: no broadcast");
    }

    #[test]
    fn clear_find_delegates_to_set_find_query() {
        let mut fake = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(3), None, None);
        fake.lv.find_mode = FindMode::Highlight;
        fake.lv.query = "ab".into();
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            fake.clear_find(&mut ctx);
        }
        assert_eq!(fake.find_query(), None, "clear_find still empties");
        assert_eq!(find_broadcasts(&out).0, 1, "clear_find still notifies once");

        // Already-empty clear_find is a no-op (the folded guard still holds).
        out.clear();
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            fake.clear_find(&mut ctx);
        }
        assert_eq!(find_broadcasts(&out).0, 0, "already-empty clear_find is a no-op");
    }
```

Note: `Event`, `Command`, `ViewId`, `FindMode`, `VecDeque`, `Rect` are already in scope in this test module (used by the neighbouring tests). If `ViewId` is not, add `use crate::view::ViewId;` — verify by compiling in Step 2.

- [ ] **Step 2: Run the tests to verify they fail**

Run:
```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test --workspace -j4 set_find_query
```
Expected: FAIL to **compile** with `no method named set_find_query found for ... FakeList` (the method does not exist yet). A compile failure is the expected "red" here.

- [ ] **Step 3: Add `set_find_query` and re-express `clear_find`**

In `src/widgets/list_viewer.rs`, replace the current `clear_find` (lines 369-380):

```rust
    /// Clear the find query — the host-callable Esc equivalent. No-op when find
    /// is `Off` or the query is already empty; otherwise fires
    /// [`Command::LIST_FIND_CHANGED`] and runs [`Self::on_query_changed`].
    fn clear_find(&mut self, ctx: &mut Context) {
        if self.lv().find_mode == FindMode::Off || self.lv().query.is_empty() {
            return;
        }
        self.lv_mut().query.clear();
        let source = self.lv().state.id();
        ctx.broadcast(Command::LIST_FIND_CHANGED, source);
        self.on_query_changed(ctx);
    }
```

with the new mutator plus the folded `clear_find`:

```rust
    /// Set the find query from an external source — e.g. a host `InputLine` that
    /// owns the text and drives an unfocused list's incremental find (a combobox).
    /// The mirror image of typed find, minus the keystrokes: a whole-string setter
    /// because a text source always holds the complete string.
    ///
    /// No-op when find mode is [`FindMode::Off`] or the query is unchanged;
    /// otherwise replaces the query, fires [`Command::LIST_FIND_CHANGED`] (source =
    /// this list) and runs [`Self::on_query_changed`] — the same change tail
    /// keystroke find runs. Passing `""` is exactly [`Self::clear_find`] reached
    /// through the same door, so a host need not special-case the empty field.
    fn set_find_query(&mut self, query: &str, ctx: &mut Context) {
        if self.lv().find_mode == FindMode::Off || self.lv().query == query {
            return;
        }
        self.lv_mut().query = query.to_string();
        find_notify(self, ctx);
    }

    /// Clear the find query — the host-callable Esc equivalent. Equivalent to
    /// `set_find_query("", ctx)`: no-op when find is `Off` or already empty,
    /// else fires [`Command::LIST_FIND_CHANGED`] and runs [`Self::on_query_changed`].
    fn clear_find(&mut self, ctx: &mut Context) {
        self.set_find_query("", ctx);
    }
```

- [ ] **Step 4: Run the tests to verify they pass**

Run:
```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test --workspace -j4 set_find_query
cargo test --workspace -j4 clear_find_delegates
cargo test --workspace -j4 find
```
Expected: all PASS (new tests green; the Task 1 keystroke/find tests still green).

- [ ] **Step 5: Roll the CHANGELOG**

In `CHANGELOG.md`, under `## Unreleased` → `### New` (line 13), add:

```markdown
- `ListViewer::set_find_query` — host-callable find-query setter so an external
  text source (e.g. an `InputLine` above the list) can drive the list's
  incremental find without the list being focused. `clear_find` is now
  `set_find_query("")`.
```

- [ ] **Step 6: Full gate + commit**

Run:
```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test  --workspace -j4
cargo clippy --workspace --all-targets -j4 -- -D warnings
cargo fmt --all --check
```
Expected: all PASS, no warnings, fmt clean. Then:

```bash
git add src/widgets/list_viewer.rs CHANGELOG.md
git commit -m "ListViewer: add set_find_query host-callable find input

Adds set_find_query(query, ctx) so a source outside the list's keystroke
handler (e.g. an InputLine driving a combobox) can set the find query. Guards
on Off-mode and unchanged text, then runs the shared find_notify tail. clear_find
folds into set_find_query(\"\"), collapsing the last inline copy of the tail so
keystroke/external parity is structural. rstv-original extension.

Co-Authored-By: <project trailer>"
```

---

### Task 3: Concrete `ListBox` Filter-mode + typed-vs-set parity tests

The shared-core tests (Task 2) use `FakeList`, which does not override `on_query_changed`, so they don't exercise self-filtering. Add behavioural coverage on the concrete `ListBox` (which overrides `on_query_changed` to narrow its view): `set_find_query` must narrow exactly as a typed query does, and a query reached by keystrokes must leave identical state to the same query reached by `set_find_query`.

**Files:**
- Modify: `src/widgets/list_box.rs` test module (add after `list_box_self_filter_clamps_focus`, currently ending line 1273)

**Interfaces:**
- Consumes: `ListBox::new(Rect, num_cols, h, v)`, `.with_find(FindMode::Filter)`, `ListBox::new_list(Vec<String>, &mut Context)`, `ListViewer::set_find_query`, `ListViewer::on_query_changed`, `ListBox::lv.range`, `ListBox::get_text(i32)`. All already used by neighbouring tests in this module.
- Produces: two tests only; no production change.

- [ ] **Step 1: Write the failing tests**

In the `src/widgets/list_box.rs` test module, add after `list_box_self_filter_clamps_focus` (after line 1273):

```rust
    #[test]
    fn set_find_query_narrows_like_typed_query() {
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];
        let mut lb =
            ListBox::new(Rect::new(0, 0, 14, 5), 1, None, None).with_find(FindMode::Filter);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            lb.new_list(
                vec!["apple".into(), "banana".into(), "grape".into(), "orange".into()],
                &mut ctx,
            );
        }
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            lb.set_find_query("an", &mut ctx);
        }
        assert_eq!(lb.lv.range, 2, "set_find_query narrows via on_query_changed");
        assert_eq!(lb.get_text(0), "banana");
        assert_eq!(lb.get_text(1), "orange", "insertion order preserved");

        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            lb.set_find_query("", &mut ctx);
        }
        assert_eq!(lb.lv.range, 4, "empty set_find_query restores the full source");
        assert_eq!(lb.get_text(0), "apple");
    }

    #[test]
    fn typed_and_set_find_query_reach_identical_state() {
        let src: Vec<String> =
            vec!["apple".into(), "banana".into(), "grape".into(), "orange".into()];

        // Path A: reach "an" by keystrokes.
        let mut out_a = VecDeque::new();
        let mut timers_a = crate::timer::TimerQueue::new();
        let mut deferred_a = vec![];
        let mut lb_a =
            ListBox::new(Rect::new(0, 0, 14, 5), 1, None, None).with_find(FindMode::Filter);
        {
            let mut ctx = make_ctx(&mut out_a, &mut timers_a, &mut deferred_a);
            lb_a.new_list(src.clone(), &mut ctx);
        }
        for c in ['a', 'n'] {
            let mut ev = key_ev(Key::Char(c));
            let mut ctx = make_ctx(&mut out_a, &mut timers_a, &mut deferred_a);
            lb_a.handle_event(&mut ev, &mut ctx);
        }

        // Path B: reach "an" by set_find_query.
        let mut out_b = VecDeque::new();
        let mut timers_b = crate::timer::TimerQueue::new();
        let mut deferred_b = vec![];
        let mut lb_b =
            ListBox::new(Rect::new(0, 0, 14, 5), 1, None, None).with_find(FindMode::Filter);
        {
            let mut ctx = make_ctx(&mut out_b, &mut timers_b, &mut deferred_b);
            lb_b.new_list(src.clone(), &mut ctx);
        }
        {
            let mut ctx = make_ctx(&mut out_b, &mut timers_b, &mut deferred_b);
            lb_b.set_find_query("an", &mut ctx);
        }

        // Identical query, find_query(), and narrowed view.
        assert_eq!(lb_a.find_query(), lb_b.find_query(), "same find_query()");
        assert_eq!(lb_a.lv.query, lb_b.lv.query, "same raw query");
        assert_eq!(lb_a.lv.range, lb_b.lv.range, "same narrowed range");
        let rows_a: Vec<String> = (0..lb_a.lv.range).map(|i| lb_a.get_text(i)).collect();
        let rows_b: Vec<String> = (0..lb_b.lv.range).map(|i| lb_b.get_text(i)).collect();
        assert_eq!(rows_a, rows_b, "same visible rows");
    }
```

Note: `key_ev`, `make_ctx`, `Key`, `FindMode`, `VecDeque`, `Rect` are already used by neighbouring tests in this module. If `key_ev` is defined only in the `list_viewer` test module and not here, add a local `key_ev` mirroring it (`Event::key(KeyEvent::from(k))` — copy the exact body from `list_viewer.rs:1664`). Verify by compiling in Step 2.

- [ ] **Step 2: Run the tests to verify they fail**

Run:
```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test --workspace -j4 -p tvision-rs list_box 2>&1 | tail -30
```
Expected: FAIL — either a compile error for a missing `key_ev` (fix per the Step 1 note, then re-run) or, once compiling, the new tests should actually **pass** already since Task 2 landed the production code. If they pass on the first compile, that is acceptable: this task is behavioural verification of already-landed code (there is no new production code to write), so record the pass and proceed. If they *fail* on an assertion, treat it as a real bug in Task 2 and fix `set_find_query`/`on_query_changed` before continuing.

- [ ] **Step 3: Confirm green + full gate**

Run:
```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test  --workspace -j4
cargo clippy --workspace --all-targets -j4 -- -D warnings
cargo fmt --all --check
```
Expected: all PASS, no warnings, fmt clean.

- [ ] **Step 4: Commit**

```bash
git add src/widgets/list_box.rs
git commit -m "ListBox: test set_find_query narrows + typed-vs-set parity

Filter-mode behavioural coverage for the new host-callable path: set_find_query
narrows exactly like a typed query, and a query reached by keystrokes leaves
identical query/find_query/narrowed-view state to the same query reached by
set_find_query. Verification only; no production change.

Co-Authored-By: <project trailer>"
```

---

## Notes for the integrator

- **Focus is out of scope** (spec §Focus policy). `set_find_query` sets the query and nothing else; the host owns focus via the existing `focus_item_num`. Do **not** add `focus_find_match` — it is deferred until a second consumer needs it.
- **No `find_route_key` / `draw` / `filtered_view` / `LIST_FIND_CHANGED` change.** The only touch to existing production code is Task 1's tail extraction and Task 2's `clear_find` body; both are behaviour-preserving and guarded by the unchanged keystroke tests.
- **edaptor consumer is not part of this crate** (spec §Consumer sketch) — it ships separately with its own task. Nothing in edaptor is touched here.
- Whole-tree redraw (D-rule) means an externally-driven query re-highlights on the next pump loop with no extra `invalidate` call needed.

## Self-review checklist (done)

- **Spec coverage:** Core model `set_find_query` → Task 2. Semantics 1–4 (Off no-op / change-guard / notify-on-change / empty-clears) → Task 2 Steps 1+3 tests. Layering (`find_notify` extraction, `clear_find` fold, no concrete-widget change) → Tasks 1+2. Testing §: shared-core cases → Task 2; Filter-mode + typed-vs-set parity → Task 3. Focus policy / non-goals → "Notes for the integrator". ✅
- **Placeholders:** none — every code step shows full code; the only `<project trailer>` placeholders are commit-message trailers the worker copies from a recent commit (verify with `git log -1`). ✅
- **Type consistency:** `find_notify(&mut L, &mut Context)`, `set_find_query(&mut self, &str, &mut Context)`, `clear_find(&mut self, &mut Context)` used identically across Tasks 1–3; `find_broadcasts` helper defined once in Task 2 and reused. ✅
