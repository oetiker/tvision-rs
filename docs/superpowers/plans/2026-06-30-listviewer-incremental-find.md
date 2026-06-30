# `ListViewer` Incremental Find-and-Highlight Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add an opt-in *find mode* to the list widgets — type while a list is focused to accumulate a query that highlights the matched substring in every row and (optionally) self-filters the visible rows — generalising the existing type-to-search lookup, so consumers no longer need a separate search `InputLine`.

**Architecture:** Find state (`find_mode: FindMode`, `query: String`) lives on the shared `ListViewerState`, so the shared `draw` can reach it via a defaulted `ListViewer::find_query()`. The shared `handle_event` grows a front-of-`KeyDown` branch that routes printable / Backspace / Esc into the query (firing a `LIST_FIND_CHANGED` broadcast — the same notify-by-broadcast `ScrollBar` uses — and a `on_query_changed` hook), while non-query keys fall through to navigation unchanged; `sorted_handle_event` skips the classic lookup entirely when find mode is on. The shared `draw` splits each row at the first case-insensitive match and paints the match span in the list's `selected` accent role. Concrete `ListBox` / `SortedListBox` keep the host-supplied set as a `source` and, in `Filter` mode, narrow the displayed `items` to it in `on_query_changed`.

**Tech Stack:** Rust (Cargo workspace `tvision-rs` + `tvision-rs-macros`), `insta` snapshot tests on the `HeadlessBackend`.

**Design note:** `docs/superpowers/specs/2026-06-30-listviewer-incremental-find-design.md` — consult it for rationale; this plan is the line-level recipe.

## Global Constraints

- `export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target` before any cargo command (artifacts land there, not `./target`).
- Build/test on **at most 4 cores**: prefix cargo with `CARGO_BUILD_JOBS=4` and pass `-- --test-threads=4` to tests.
- Verification gate for every task (all must pass):
  - `cargo test --workspace -j4 -- --test-threads=4`
  - `cargo clippy --workspace --all-targets -- -D warnings`
  - `cargo fmt --all --check`
- English for all code/comments/identifiers.
- Roll `CHANGELOG.md` (`## Unreleased` → `### New`) for the user-visible feature (Task 6).
- Snapshot tests: a new `insta::assert_snapshot!` produces a `.snap.new`; **review it** (`cargo insta review`, or read the file) and rename to `.snap` before committing — eyeball the whole frame, not just the highlighted glyph (snapshot-at-origin lesson).
- Faithful-by-default: this is a deliberate rstv *extension* alongside the faithful port (precedent: `RegexValidator` next to the picture-mask port). The default (`FindMode::Off`) leaves every existing consumer's behaviour byte-identical.
- Commit messages end with: `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`

---

### Task 1: `FindMode` enum, find state on `ListViewerState`, `LIST_FIND_CHANGED` command, trait accessors

**Files:**
- Modify: `src/widgets/list_viewer.rs` (add `FindMode`, two `ListViewerState` fields + init, three `ListViewer` trait methods)
- Modify: `src/command.rs:290` (add `LIST_FIND_CHANGED` after `LIST_ITEM_SELECTED`)
- Modify: `src/widgets/mod.rs:49` (export `FindMode`)
- Modify: `src/lib.rs:144` (re-export `FindMode`)
- Test: `src/widgets/list_viewer.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Produces:
  - `pub enum FindMode { Off, Highlight, Filter }` (in `widgets::list_viewer`, re-exported at crate root)
  - `ListViewerState { …, pub find_mode: FindMode, pub query: String }`
  - `ListViewer::find_query(&self) -> Option<&str>` (default impl)
  - `ListViewer::clear_find(&mut self, ctx: &mut Context)` (default impl)
  - `ListViewer::on_query_changed(&mut self, ctx: &mut Context)` (default no-op)
  - `Command::LIST_FIND_CHANGED`

- [ ] **Step 1: Add the `LIST_FIND_CHANGED` command**

In `src/command.rs`, immediately after the `LIST_ITEM_SELECTED` const (line 290):

```rust
    /// The find query changed (incremental find-and-highlight); `source` is the
    /// list viewer's `ViewId`. Mirrors [`SCROLL_BAR_CHANGED`]'s notify-by-
    /// broadcast — a parent filters on `source` to drive an external search
    /// (e.g. submit an async query and re-feed results).
    pub const LIST_FIND_CHANGED: Command = Command("tv.list_find_changed");
```

- [ ] **Step 2: Add the `FindMode` enum**

In `src/widgets/list_viewer.rs`, just before the `ListViewerState` struct (line 111):

```rust
/// Find-and-highlight mode for a list. Opt-in; the default [`FindMode::Off`]
/// keeps the classic type-to-search prefix lookup unchanged.
///
/// # Turbo Vision heritage
///
/// An rstv extension on top of the faithful `TListViewer` lookup — see the
/// design note `docs/superpowers/specs/2026-06-30-listviewer-incremental-find-design.md`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FindMode {
    /// Off — the classic prefix lookup (today's behaviour).
    Off,
    /// Query + highlight only; the host supplies and filters the rows.
    Highlight,
    /// Query + highlight + self-filter: the list narrows its own source set.
    Filter,
}
```

- [ ] **Step 3: Add the two find fields to `ListViewerState`**

In `src/widgets/list_viewer.rs`, in the `ListViewerState` struct (ends line 166), add after the `track` field:

```rust
    /// Find mode (opt-in). `Off` keeps the classic prefix lookup; the other
    /// variants enable the accumulated-query find-and-highlight.
    pub find_mode: FindMode,
    /// The accumulated find query (find mode only; always empty when `Off`).
    pub query: String,
```

And in `ListViewerState::new` (the struct literal at lines 196–207), add after `track: None,`:

```rust
            find_mode: FindMode::Off,
            query: String::new(),
```

- [ ] **Step 4: Add the three trait methods to `ListViewer`**

In `src/widgets/list_viewer.rs`, inside the `pub trait ListViewer: View` block, after `on_focus_changed` (line 320) and before the closing `}` (line 321):

```rust
    /// The current non-empty find query, or `None` when find mode is `Off` or
    /// the query is empty. The shared [`draw`] reads this to highlight matches;
    /// hosts read it to mirror the query elsewhere.
    fn find_query(&self) -> Option<&str> {
        let lv = self.lv();
        if lv.find_mode == FindMode::Off || lv.query.is_empty() {
            None
        } else {
            Some(&lv.query)
        }
    }

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

    /// Hook fired after the find query changes (default: no-op). A self-filtering
    /// concrete widget overrides it to re-derive its visible rows from its
    /// source. Called by the shared `handle_event` and by [`Self::clear_find`].
    fn on_query_changed(&mut self, _ctx: &mut Context) {}
```

- [ ] **Step 5: Export `FindMode`**

In `src/widgets/mod.rs:49`, change:

```rust
pub use list_viewer::{ListRoles, ListViewer, ListViewerState};
```
to:
```rust
pub use list_viewer::{FindMode, ListRoles, ListViewer, ListViewerState};
```

In `src/lib.rs:144`, add `FindMode` to the existing list re-export group (it imports `ListBox, ListRoles, ListViewer, ListViewerState` on that line — insert `FindMode,` alphabetically before `LimitMode`/`ListBox`):

```rust
    EditWindow, Editor, FindMode, InputLine, LimitMode, ListBox, ListRoles, ListViewer,
    ListViewerState,
```
(Keep the rest of the existing line intact; only insert `FindMode,`.)

- [ ] **Step 6: Write the failing test for `find_query`**

In the `#[cfg(test)] mod tests` of `src/widgets/list_viewer.rs`, add (the test module already imports `Command`, `Rect`, `FakeList`, `items`; add `FindMode` to the `use super::*;`-reachable names — it is in `super`, so no new `use` is needed):

```rust
    #[test]
    fn find_query_reflects_mode_and_emptiness() {
        let mut fake = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(3), None, None);
        assert_eq!(fake.find_query(), None, "Off → None");
        fake.lv.find_mode = FindMode::Highlight;
        assert_eq!(fake.find_query(), None, "empty query → None");
        fake.lv.query = "ab".into();
        assert_eq!(fake.find_query(), Some("ab"));
        fake.lv.find_mode = FindMode::Off;
        assert_eq!(fake.find_query(), None, "Off overrides a non-empty query");
    }
```

- [ ] **Step 7: Run the test to verify it passes**

Run: `CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target CARGO_BUILD_JOBS=4 cargo test --workspace -j4 find_query_reflects_mode_and_emptiness -- --test-threads=4`
Expected: PASS.

- [ ] **Step 8: Run the full gate**

Run all three Global-Constraint gate commands. Expected: PASS / no warnings.

- [ ] **Step 9: Commit**

```bash
git add src/command.rs src/widgets/list_viewer.rs src/widgets/mod.rs src/lib.rs
git commit -m "feat(list): FindMode + find state on ListViewerState"
```

---

### Task 2: `find_match` + shared self-filter helpers (`filtered_view`, `apply_view_len`)

**Files:**
- Modify: `src/widgets/list_viewer.rs` (add `find_match` + `ci_char_eq` near `ci_prefix_eq` at line 921; add `filtered_view` + `apply_view_len` near the other view free fns, e.g. after `set_range` at line 400)
- Test: `src/widgets/list_viewer.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `FindMode` (Task 1), `set_range`/`focus_item`/`focus_item_num`.
- Produces:
  - `pub(crate) fn find_match(text: &str, query: &str) -> Option<(usize, usize)>` — `[start, end)` **char** indices of the first case-insensitive occurrence, or `None` (also `None` for an empty query or a query longer than the text). Char-indexed so each query char maps to exactly one text char (no Unicode case-folding length skew).
  - `pub(crate) fn filtered_view(source: &[String], mode: FindMode, query: &str) -> Vec<String>` — the displayed view of `source`: the full source unless `Filter` mode with a non-empty query narrows it (case-insensitive substring, order preserved).
  - `pub(crate) fn apply_view_len<L: ListViewer + ?Sized>(this: &mut L, len: i32, reset_focus: bool, ctx: &mut Context)` — republish `len` as the range and place focus (top when `reset_focus`, else clamp the existing focus into range). The shared body behind both concrete widgets' `rebuild_view`, so the filter→range→focus logic is written once.

- [ ] **Step 1: Write the failing test**

In `src/widgets/list_viewer.rs` test module:

```rust
    #[test]
    fn find_match_first_occurrence_char_indexed() {
        assert_eq!(find_match("banana", "an"), Some((1, 3)), "middle");
        assert_eq!(find_match("banana", "ba"), Some((0, 2)), "start");
        assert_eq!(find_match("banana", "na"), Some((2, 4)), "first of repeats");
        assert_eq!(find_match("Banana", "an"), Some((1, 3)), "case-insensitive text");
        assert_eq!(find_match("banana", "BAN"), Some((0, 3)), "case-insensitive query");
        assert_eq!(find_match("banana", "xyz"), None, "no match");
        assert_eq!(find_match("ab", ""), None, "empty query");
        assert_eq!(find_match("a", "abc"), None, "query longer than text");
        assert_eq!(find_match("café", "é"), Some((3, 4)), "multibyte char index");
        assert_eq!(find_match("naïve", "ï"), Some((2, 3)), "multibyte mid-word");
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target CARGO_BUILD_JOBS=4 cargo test --workspace -j4 find_match_first_occurrence -- --test-threads=4`
Expected: FAIL to compile — `cannot find function find_match`.

- [ ] **Step 3: Implement the helpers**

In `src/widgets/list_viewer.rs`, near `ci_prefix_eq` (line 921), add:

```rust
/// Case-insensitive equality of two `char`s. Cheap ASCII path first, then a
/// Unicode `to_lowercase` sequence compare (per-char, so indices stay aligned).
fn ci_char_eq(a: char, b: char) -> bool {
    a == b || a.eq_ignore_ascii_case(&b) || a.to_lowercase().eq(b.to_lowercase())
}

/// The half-open `[start, end)` **char** index range of the first
/// case-insensitive occurrence of `query` in `text`, or `None`. Returns `None`
/// for an empty query or a query longer than `text`. Char-indexed: each query
/// char matches exactly one text char, so the range is safe to slice by `char`.
pub(crate) fn find_match(text: &str, query: &str) -> Option<(usize, usize)> {
    let q: Vec<char> = query.chars().collect();
    if q.is_empty() {
        return None;
    }
    let t: Vec<char> = text.chars().collect();
    if q.len() > t.len() {
        return None;
    }
    'outer: for start in 0..=(t.len() - q.len()) {
        for k in 0..q.len() {
            if !ci_char_eq(t[start + k], q[k]) {
                continue 'outer;
            }
        }
        return Some((start, start + q.len()));
    }
    None
}
```

- [ ] **Step 4: Add the shared self-filter helpers**

In `src/widgets/list_viewer.rs`, after `set_range` (line 400), add:

```rust
/// The displayed view of `source` for a find mode/query: the full source unless
/// `Filter` mode with a non-empty query narrows it to the rows containing the
/// query (case-insensitive substring), preserving `source` order.
pub(crate) fn filtered_view(source: &[String], mode: FindMode, query: &str) -> Vec<String> {
    if mode == FindMode::Filter && !query.is_empty() {
        source
            .iter()
            .filter(|s| find_match(s, query).is_some())
            .cloned()
            .collect()
    } else {
        source.to_vec()
    }
}

/// Republish `len` as the range and place focus: to the top when `reset_focus`
/// (a fresh `new_list`), else clamp the existing focus into the new range (a
/// query change). The shared tail of both concrete widgets' `rebuild_view`.
pub(crate) fn apply_view_len<L: ListViewer + ?Sized>(
    this: &mut L,
    len: i32,
    reset_focus: bool,
    ctx: &mut Context,
) {
    set_range(this, len, ctx);
    if reset_focus {
        if len > 0 {
            focus_item(this, 0, ctx);
        }
    } else {
        let f = this.lv().focused.min(len - 1).max(0);
        focus_item_num(this, f, ctx);
    }
}
```

- [ ] **Step 5: Add a test for `filtered_view`**

In `src/widgets/list_viewer.rs` test module:

```rust
    #[test]
    fn filtered_view_narrows_only_in_filter_mode() {
        let src = vec!["apple".to_string(), "banana".into(), "orange".into()];
        assert_eq!(filtered_view(&src, FindMode::Off, "an"), src, "Off: full source");
        assert_eq!(filtered_view(&src, FindMode::Highlight, "an"), src, "Highlight: full source");
        assert_eq!(filtered_view(&src, FindMode::Filter, ""), src, "empty query: full source");
        assert_eq!(
            filtered_view(&src, FindMode::Filter, "an"),
            vec!["banana".to_string(), "orange".into()],
            "Filter narrows, order preserved"
        );
    }
```

- [ ] **Step 6: Run to verify it passes**

Run: `CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target CARGO_BUILD_JOBS=4 cargo test --workspace -j4 find_match_first_occurrence filtered_view_narrows -- --test-threads=4`
Expected: PASS.

- [ ] **Step 7: Run the full gate** (all three commands). Expected: PASS.

- [ ] **Step 8: Commit**

```bash
git add src/widgets/list_viewer.rs
git commit -m "feat(list): find_match + filtered_view/apply_view_len helpers"
```

---

### Task 3: Find key-routing in shared `handle_event` + lookup bypass in `sorted_handle_event`

**Files:**
- Modify: `src/widgets/list_viewer.rs:79` (add `KeyEvent` to the event import)
- Modify: `src/widgets/list_viewer.rs` (add `find_route_key` + `find_after_change` free fns; insert a branch at the top of the `Event::KeyDown(ke)` arm at line 712; add a bypass guard in `sorted_handle_event` at line 960)
- Test: `src/widgets/list_viewer.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `Command::LIST_FIND_CHANGED`, `ListViewer::on_query_changed`, `ListViewerState::{find_mode, query}` (Task 1).
- Produces: find keystroke behaviour — printable chars (incl. Space) append; Backspace deletes; Esc clears (consumed) while non-empty, propagates while empty; every change fires `LIST_FIND_CHANGED` + `on_query_changed`; non-query keys navigate as before.

- [ ] **Step 1: Add `KeyEvent` to the event import**

In `src/widgets/list_viewer.rs:79`, change:
```rust
use crate::event::{Event, Key, ctrl_to_arrow};
```
to:
```rust
use crate::event::{Event, Key, KeyEvent, ctrl_to_arrow};
```

- [ ] **Step 2: Write the failing tests**

In `src/widgets/list_viewer.rs` test module (uses the existing `make_ctx`, `key_ev`, `items`, `FakeList`, `Command`, `VecDeque`, `TimerQueue` helpers already present for the other event tests):

```rust
    #[test]
    fn find_mode_accumulates_query_and_broadcasts_on_change() {
        let mut fake = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(5), None, None);
        fake.lv.find_mode = FindMode::Highlight;
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];
        for c in ['a', 'b', ' ', 'c'] {
            let mut ev = key_ev(Key::Char(c));
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            fake.handle_event(&mut ev, &mut ctx);
            assert!(ev.is_nothing(), "find key consumed");
        }
        assert_eq!(fake.lv.query, "ab c", "Space appends to the query");
        let broadcasts = out
            .iter()
            .filter(|e| matches!(e,
                Event::Broadcast { command, .. } if *command == Command::LIST_FIND_CHANGED))
            .count();
        assert_eq!(broadcasts, 4, "one broadcast per query change");
    }

    #[test]
    fn find_backspace_and_esc_behaviour() {
        let mut fake = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(5), None, None);
        fake.lv.find_mode = FindMode::Highlight;
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];

        // Backspace pops; consumed.
        fake.lv.query = "ab".into();
        {
            let mut ev = key_ev(Key::Backspace);
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            fake.handle_event(&mut ev, &mut ctx);
            assert!(ev.is_nothing());
        }
        assert_eq!(fake.lv.query, "a");

        // Backspace on empty: no change, no broadcast, NOT consumed (propagates).
        fake.lv.query.clear();
        out.clear();
        {
            let mut ev = key_ev(Key::Backspace);
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            fake.handle_event(&mut ev, &mut ctx);
            assert!(!ev.is_nothing(), "empty Backspace propagates");
        }
        assert!(out.is_empty(), "no broadcast on a no-op Backspace");

        // Esc with a query: clears + consumes.
        fake.lv.query = "ab".into();
        {
            let mut ev = key_ev(Key::Esc);
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            fake.handle_event(&mut ev, &mut ctx);
            assert!(ev.is_nothing(), "Esc with a query is consumed");
        }
        assert_eq!(fake.lv.query, "");

        // Esc with empty query: propagates (a host dialog can still close).
        {
            let mut ev = key_ev(Key::Esc);
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            fake.handle_event(&mut ev, &mut ctx);
            assert!(!ev.is_nothing(), "empty-query Esc propagates");
        }
    }

    #[test]
    fn find_mode_lets_arrows_navigate_and_keeps_query() {
        let mut fake = FakeList::new(Rect::new(0, 0, 10, 5), 1, items(5), None, None);
        fake.lv.find_mode = FindMode::Highlight;
        fake.lv.query = "a".into();
        fake.lv.focused = 0;
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];
        let mut ev = key_ev(Key::Down);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            fake.handle_event(&mut ev, &mut ctx);
        }
        assert_eq!(fake.lv.focused, 1, "Down still navigates in find mode");
        assert_eq!(fake.lv.query, "a", "query persists across navigation");
    }
```

- [ ] **Step 3: Run to verify they fail**

Run: `CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target CARGO_BUILD_JOBS=4 cargo test --workspace -j4 find_mode_accumulates find_backspace find_mode_lets -- --test-threads=4`
Expected: FAIL (query not accumulated, no broadcast — the find branch does not exist yet).

- [ ] **Step 4: Add the routing free functions**

In `src/widgets/list_viewer.rs`, just before `pub fn handle_event` (line 532), add:

```rust
/// Route a keystroke into the find query when find mode is active. Returns
/// `true` if the key was a find key (and the event was consumed), `false` if it
/// is not a find key (the caller continues with normal navigation). Backspace
/// on an empty query and Esc on an empty query both return `false` so they
/// propagate (an empty-query Esc lets a host dialog close).
fn find_route_key<L: ListViewer + ?Sized>(
    this: &mut L,
    ke: KeyEvent,
    ev: &mut Event,
    ctx: &mut Context,
) -> bool {
    match ke.key {
        Key::Char(c) if !ke.modifiers.ctrl && !ke.modifiers.alt => {
            this.lv_mut().query.push(c);
            find_after_change(this, ev, ctx);
            true
        }
        Key::Backspace => {
            if this.lv().query.is_empty() {
                return false;
            }
            this.lv_mut().query.pop();
            find_after_change(this, ev, ctx);
            true
        }
        Key::Esc => {
            if this.lv().query.is_empty() {
                return false;
            }
            this.lv_mut().query.clear();
            find_after_change(this, ev, ctx);
            true
        }
        _ => false,
    }
}

/// Common tail after the query changes: broadcast the change (self as `source`,
/// mirroring `select_item` / `ScrollBar`), run the self-filter hook, consume.
fn find_after_change<L: ListViewer + ?Sized>(this: &mut L, ev: &mut Event, ctx: &mut Context) {
    let source = this.lv().state.id();
    ctx.broadcast(Command::LIST_FIND_CHANGED, source);
    this.on_query_changed(ctx);
    ev.clear();
}
```

- [ ] **Step 5: Insert the find branch in the `KeyDown` arm**

In `src/widgets/list_viewer.rs`, at the very top of the `Event::KeyDown(ke) => {` arm body (line 712, right after the `=> {`), insert:

```rust
            // Find mode (opt-in) intercepts query keys before navigation; a
            // non-query key (arrows, Enter, …) falls through to the nav below.
            if this.lv().find_mode != FindMode::Off && find_route_key(this, ke, ev, ctx) {
                return;
            }
```

- [ ] **Step 6: Bypass the classic lookup when find mode is on**

In `src/widgets/list_viewer.rs`, in `sorted_handle_event` (line 959), immediately after the base call `handle_event(this, ev, ctx);` (line 960), insert:

```rust
    // Find mode replaces the type-to-search prefix lookup entirely; the base
    // call above already routed any find key.
    if this.lv().find_mode != FindMode::Off {
        return;
    }
```

- [ ] **Step 7: Run the tests to verify they pass**

Run: `CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target CARGO_BUILD_JOBS=4 cargo test --workspace -j4 find_mode_accumulates find_backspace find_mode_lets -- --test-threads=4`
Expected: PASS.

- [ ] **Step 8: Run the full gate** (all three commands). Expected: PASS — confirm no regression in the existing `sorted_*` / lookup tests.

- [ ] **Step 9: Commit**

```bash
git add src/widgets/list_viewer.rs
git commit -m "feat(list): route find keystrokes; bypass lookup in find mode"
```

---

### Task 4: Shared `draw` — three-part highlight + `No match: <query>` placeholder

**Files:**
- Modify: `src/widgets/list_viewer.rs` (add `draw_highlighted` free fn; resolve an `accent` style in `draw`; replace the row text-draw line at ~864–866 and the `<empty>` placeholder line at ~867–870)
- Test: `src/widgets/list_viewer.rs` (`#[cfg(test)] mod tests`) — two snapshot tests via the existing `render` helper (line 1751)

**Interfaces:**
- Consumes: `ListViewer::find_query` (Task 1), `find_match` (Task 2).
- Produces: rows with a non-empty matching query draw the match span in the list's `selected` accent role; a query that filters the view empty renders `No match: <query>` instead of `<empty>`.

- [ ] **Step 1: Write the failing snapshot tests**

In `src/widgets/list_viewer.rs` test module (the `render(&mut FakeList, w: u16, h: u16) -> String` helper at line 1751 is in scope):

```rust
    #[test]
    fn snapshot_find_highlights_first_match() {
        let mut fake = FakeList::new(
            Rect::new(0, 0, 14, 4),
            1,
            vec!["banana".into(), "orange".into(), "grape".into()],
            None,
            None,
        );
        fake.lv.state.state.selected = true;
        fake.lv.state.state.active = true;
        fake.lv.range = 3;
        fake.lv.find_mode = FindMode::Highlight;
        fake.lv.query = "an".into();
        insta::assert_snapshot!(render(&mut fake, 14, 4));
    }

    #[test]
    fn snapshot_find_empty_shows_no_match_placeholder() {
        let mut fake = FakeList::new(Rect::new(0, 0, 16, 3), 1, vec![], None, None);
        fake.lv.state.state.selected = true;
        fake.lv.state.state.active = true;
        fake.lv.range = 0;
        fake.lv.find_mode = FindMode::Filter;
        fake.lv.query = "xyz".into();
        insta::assert_snapshot!(render(&mut fake, 16, 3));
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target CARGO_BUILD_JOBS=4 cargo test --workspace -j4 snapshot_find_highlights snapshot_find_empty -- --test-threads=4`
Expected: FAIL — the generated `.snap.new` shows no accent on the match / shows `<empty>` not `No match: xyz` (no highlight code yet).

- [ ] **Step 3: Add the `draw_highlighted` helper**

In `src/widgets/list_viewer.rs`, just before `pub fn draw` (line 803), add (`Style` is `crate::color::Style`; `DrawCtx::put_str_part` returns the number of display columns it drew):

```rust
/// Draw `text` at `(x, y)` after skipping `indent` leading display columns,
/// painting the half-open **char** range `[m0, m1)` in `accent` and the rest in
/// `base`. Splits the row into before/match/after and emits up to three
/// `put_str_part` calls, threading the horizontal-scroll `indent` budget across
/// the pieces (a piece fully left of the scroll offset draws nothing and just
/// consumes the budget). The split is by `char`, so it is multibyte-safe; the
/// per-piece column accounting assumes one column per char (wide CJK glyphs in a
/// horizontally-scrolled list are the one imperfect case — acceptable: the
/// default has no h-scroll, and the classic lookup shares the assumption).
fn draw_highlighted(
    ctx: &mut DrawCtx,
    x: i32,
    y: i32,
    text: &str,
    indent: i32,
    base: crate::color::Style,
    accent: crate::color::Style,
    m0: usize,
    m1: usize,
) {
    let chars: Vec<char> = text.chars().collect();
    let before: String = chars[..m0].iter().collect();
    let matched: String = chars[m0..m1].iter().collect();
    let after: String = chars[m1..].iter().collect();
    let mut cx = x;
    let mut rem = indent;
    for (piece, style) in [(before, base), (matched, accent), (after, base)] {
        let w = piece.chars().count() as i32;
        if rem >= w {
            // Entirely scrolled off to the left; just consume the indent budget.
            rem -= w;
        } else {
            let drawn = ctx.put_str_part(cx, y, &piece, rem, style);
            cx += drawn;
            rem = 0;
        }
    }
}
```

- [ ] **Step 4: Resolve the accent style in `draw`**

In `src/widgets/list_viewer.rs`, in `draw`, just after `let empty_color = ctx.style(roles.normal_active);` (line 828), add:

```rust
    // The find-highlight accent reuses the list's `selected` role: it pops on
    // normal (cyan) and focused (green) rows; on a multi-select-selected row it
    // matches the row colour (acceptable — selection already marks that row).
    let accent = ctx.style(roles.selected);
```

- [ ] **Step 5: Replace the row text-draw line with the highlight split**

In `src/widgets/list_viewer.rs`, in the `draw` row loop, replace the existing `if item < range { … } else if i == 0 && j == 0 { … }` text block (lines ~860–870) with:

```rust
            if item < range {
                // Draw the item text from column +1, skipping `indent` leading
                // columns (the horizontal scroll offset).
                let text = this.get_text(item);
                match this.find_query().and_then(|q| find_match(&text, q)) {
                    Some((m0, m1)) => {
                        draw_highlighted(ctx, cur_col + 1, i, &text, indent, color, accent, m0, m1);
                    }
                    None => {
                        ctx.put_str_part(cur_col + 1, i, &text, indent, color);
                    }
                }
            } else if i == 0 && j == 0 {
                // Past the end of the list. With an active find query the empty
                // view means "no row survived the filter" — show the query so an
                // over-typed/mistyped query is always visible.
                match this.find_query() {
                    Some(q) => {
                        let msg = format!("No match: {q}");
                        ctx.put_str(cur_col + 1, i, &msg, empty_color);
                    }
                    None => {
                        ctx.put_str(cur_col + 1, i, EMPTY_TEXT, empty_color);
                    }
                }
            }
```

- [ ] **Step 6: Run, review the snapshots, accept**

Run: `CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target CARGO_BUILD_JOBS=4 cargo test --workspace -j4 snapshot_find_highlights snapshot_find_empty -- --test-threads=4`
Expected: FAIL with `.snap.new` files written. **Read** each `.snap.new` under `src/widgets/snapshots/`: confirm `banana`/`orange` show their `an` span in the accent colour and `grape` is plain; confirm the empty case shows `No match: xyz`. Then accept (`cargo insta accept`, or rename `.snap.new` → `.snap`). Re-run — Expected: PASS.

- [ ] **Step 7: Run the full gate** (all three commands). Expected: PASS — confirm the existing `snapshot_empty_shows_placeholder` still passes unchanged (the `None` branch is byte-identical to before).

- [ ] **Step 8: Commit**

```bash
git add src/widgets/list_viewer.rs src/widgets/snapshots/
git commit -m "feat(list): highlight find matches; No-match placeholder"
```

---

### Task 5: `ListBox` self-filter (source storage, `with_find`, `new_list`, `on_query_changed`)

**Files:**
- Modify: `src/widgets/list_box.rs` (`ListBox` struct + `new` + `new_list`; add `with_find`, a private `rebuild_view`, and an `on_query_changed` override)
- Test: `src/widgets/list_box.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: `FindMode`, `ListViewer::on_query_changed`, `find_match` (Tasks 1–2), `list_viewer::{set_range, focus_item, focus_item_num}`.
- Produces:
  - `ListBox::with_find(self, mode: FindMode) -> Self` (builder)
  - `ListBox` keeps a `source: Vec<String>`; `get_text`/`range` expose `items`, which equals the source in `Off`/`Highlight` and the query-narrowed source in `Filter`.

- [ ] **Step 1: Write the failing tests**

In `src/widgets/list_box.rs` test module. Add `FindMode` and the `ListViewer` trait to the test `use` (so `get_text`/`on_query_changed` are callable):

```rust
    use crate::widgets::list_viewer::{FindMode, ListViewer};
```
Then:

```rust
    #[test]
    fn list_box_self_filter_narrows_and_restores() {
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];
        let mut lb = ListBox::new(Rect::new(0, 0, 14, 5), 1, None, None).with_find(FindMode::Filter);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            lb.new_list(
                vec!["apple".into(), "banana".into(), "grape".into(), "orange".into()],
                &mut ctx,
            );
        }
        assert_eq!(lb.lv.range, 4, "empty query shows the full source");

        lb.lv.query = "an".into();
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            lb.on_query_changed(&mut ctx);
        }
        assert_eq!(lb.lv.range, 2, "only rows containing 'an' survive");
        assert_eq!(lb.get_text(0), "banana");
        assert_eq!(lb.get_text(1), "orange", "insertion order preserved");

        lb.lv.query.clear();
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            lb.on_query_changed(&mut ctx);
        }
        assert_eq!(lb.lv.range, 4, "clearing the query restores the full source");
        assert_eq!(lb.get_text(0), "apple");
    }

    #[test]
    fn list_box_self_filter_clamps_focus() {
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];
        let mut lb = ListBox::new(Rect::new(0, 0, 14, 5), 1, None, None).with_find(FindMode::Filter);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            lb.new_list(
                vec!["apple".into(), "banana".into(), "grape".into(), "orange".into()],
                &mut ctx,
            );
        }
        lb.lv.focused = 3;
        lb.lv.query = "an".into();
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            lb.on_query_changed(&mut ctx);
        }
        assert!(lb.lv.focused <= 1, "focus clamped into the narrowed range");
    }
```

- [ ] **Step 2: Run to verify they fail**

Run: `CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target CARGO_BUILD_JOBS=4 cargo test --workspace -j4 list_box_self_filter -- --test-threads=4`
Expected: FAIL to compile — `no method named with_find`.

- [ ] **Step 3: Add the `source` field**

In `src/widgets/list_box.rs`, the `ListBox` struct (lines 51–54) becomes:

```rust
pub struct ListBox {
    lv: ListViewerState,
    /// The displayed rows (the source narrowed by the query in `Filter` mode).
    items: Vec<String>,
    /// The full host-supplied set; `items` is derived from this.
    source: Vec<String>,
}
```

In `ListBox::new` (lines 71–81), add `source: Vec::new(),` to the struct literal alongside `items`.

- [ ] **Step 4: Add `with_find` and `rebuild_view`, and rewrite `new_list`**

In `src/widgets/list_box.rs`, in `impl ListBox`, add the builder near `new`:

```rust
    /// Enable find mode (default [`FindMode::Off`] keeps the classic lookup).
    /// `Filter` makes the list narrow its own source by the query; `Highlight`
    /// only owns the query + highlight (the host supplies the rows).
    pub fn with_find(mut self, mode: crate::widgets::list_viewer::FindMode) -> Self {
        self.lv.find_mode = mode;
        self
    }
```

Replace `new_list` (lines 95–102) with:

```rust
    pub fn new_list(&mut self, items: Vec<String>, ctx: &mut Context) {
        self.source = items;
        self.rebuild_view(ctx, true);
    }

    /// Re-derive `items` from `source` (narrowing by the query in `Filter`
    /// mode), then republish the range and place focus via the shared helpers.
    fn rebuild_view(&mut self, ctx: &mut Context, reset_focus: bool) {
        self.items =
            list_viewer::filtered_view(&self.source, self.lv.find_mode, &self.lv.query);
        let len = self.items.len() as i32;
        list_viewer::apply_view_len(self, len, reset_focus, ctx);
    }
```

- [ ] **Step 5: Override `on_query_changed`**

In `src/widgets/list_box.rs`, in `impl ListViewer for ListBox` (the block starting at line ~120 where `lv`/`lv_mut`/`get_text` live), add:

```rust
    fn on_query_changed(&mut self, ctx: &mut Context) {
        if self.lv.find_mode == crate::widgets::list_viewer::FindMode::Filter {
            self.rebuild_view(ctx, false);
        }
    }
```

- [ ] **Step 6: Run to verify the tests pass**

Run: `CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target CARGO_BUILD_JOBS=4 cargo test --workspace -j4 list_box_self_filter -- --test-threads=4`
Expected: PASS.

- [ ] **Step 7: Run the full gate** (all three commands). Expected: PASS — the existing `ListBox` snapshot/event tests still pass (they set `items` directly and never call `on_query_changed`, and `Off` mode leaves `new_list` behaviour identical).

- [ ] **Step 8: Commit**

```bash
git add src/widgets/list_box.rs
git commit -m "feat(list): ListBox self-filter (source + with_find + on_query_changed)"
```

---

### Task 6: `SortedListBox` self-filter + CHANGELOG

**Files:**
- Modify: `src/widgets/list_box.rs` (`SortedListBox` struct + `new` + `new_list`; add `with_find`, `rebuild_view`, `on_query_changed`)
- Modify: `CHANGELOG.md` (`## Unreleased` → `### New`)
- Test: `src/widgets/list_box.rs` (`#[cfg(test)] mod tests`)

**Interfaces:**
- Consumes: same as Task 5, plus `ci_cmp` (the existing case-insensitive comparator at line 208).
- Produces: `SortedListBox::with_find(self, mode) -> Self`; `SortedListBox` filters its already-sorted `source`, so the narrowed view stays in sorted order.

- [ ] **Step 1: Write the failing test**

In `src/widgets/list_box.rs` test module (the `FindMode`/`ListViewer` `use` from Task 5 is already present):

```rust
    #[test]
    fn sorted_list_box_self_filter_keeps_sorted_order() {
        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred = vec![];
        let mut slb =
            SortedListBox::new(Rect::new(0, 0, 14, 5), 1, None, None).with_find(FindMode::Filter);
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            slb.new_list(
                vec!["orange".into(), "apple".into(), "banana".into(), "grape".into()],
                &mut ctx,
            );
        }
        // Sorted: apple, banana, grape, orange.
        assert_eq!(slb.get_text(0), "apple");
        assert_eq!(slb.lv.range, 4);

        slb.lv.query = "an".into();
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            slb.on_query_changed(&mut ctx);
        }
        // Containing "an", in sorted order: banana, orange.
        assert_eq!(slb.lv.range, 2);
        assert_eq!(slb.get_text(0), "banana");
        assert_eq!(slb.get_text(1), "orange");

        slb.lv.query.clear();
        {
            let mut ctx = make_ctx(&mut out, &mut timers, &mut deferred);
            slb.on_query_changed(&mut ctx);
        }
        assert_eq!(slb.lv.range, 4, "clearing restores the full sorted source");
    }
```

- [ ] **Step 2: Run to verify it fails**

Run: `CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target CARGO_BUILD_JOBS=4 cargo test --workspace -j4 sorted_list_box_self_filter -- --test-threads=4`
Expected: FAIL to compile — `no method named with_find` on `SortedListBox`.

- [ ] **Step 3: Add the `source` field**

In `src/widgets/list_box.rs`, the `SortedListBox` struct (lines 234–242) gains a `source` field after `items`:

```rust
    /// The full host-supplied set, kept sorted; `items` is derived from this.
    source: Vec<String>,
```

In `SortedListBox::new` (lines 254–269), add `source: Vec::new(),` to the struct literal.

- [ ] **Step 4: Add `with_find` and `rebuild_view`, and rewrite `new_list`**

In `src/widgets/list_box.rs`, in `impl SortedListBox`, add the builder:

```rust
    /// Enable find mode (default [`FindMode::Off`] keeps the classic lookup).
    pub fn with_find(mut self, mode: crate::widgets::list_viewer::FindMode) -> Self {
        self.lv.find_mode = mode;
        self
    }
```

Replace `new_list` (lines 281–290) with:

```rust
    pub fn new_list(&mut self, mut items: Vec<String>, ctx: &mut Context) {
        items.sort_by(|a, b| ci_cmp(a, b));
        self.source = items;
        self.search_pos = -1;
        self.rebuild_view(ctx, true);
    }

    /// Re-derive `items` by narrowing the already-sorted `source` with the query
    /// (in `Filter` mode); the narrowed view stays sorted because `source` is
    /// sorted and `filtered_view` preserves order.
    fn rebuild_view(&mut self, ctx: &mut Context, reset_focus: bool) {
        self.items =
            list_viewer::filtered_view(&self.source, self.lv.find_mode, &self.lv.query);
        let len = self.items.len() as i32;
        list_viewer::apply_view_len(self, len, reset_focus, ctx);
    }
```

- [ ] **Step 5: Override `on_query_changed`**

In `src/widgets/list_box.rs`, in `impl ListViewer for SortedListBox`, add:

```rust
    fn on_query_changed(&mut self, ctx: &mut Context) {
        if self.lv.find_mode == crate::widgets::list_viewer::FindMode::Filter {
            self.rebuild_view(ctx, false);
        }
    }
```

- [ ] **Step 6: Run to verify it passes**

Run: `CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target CARGO_BUILD_JOBS=4 cargo test --workspace -j4 sorted_list_box_self_filter -- --test-threads=4`
Expected: PASS.

- [ ] **Step 7: Roll the CHANGELOG**

In `CHANGELOG.md`, under `## Unreleased` → `### New` (create the heading if absent), add:

```markdown
- `ListViewer` incremental **find mode** (`FindMode::Off`/`Highlight`/`Filter`,
  opt-in via `ListBox::with_find` / `SortedListBox::with_find`): type while a
  list is focused to accumulate a query that highlights the matched substring in
  every row and, in `Filter` mode, narrows the list to its own source. Exposes
  `ListViewer::find_query` / `clear_find`, broadcasts `Command::LIST_FIND_CHANGED`
  (self as `source`), and shows `No match: <query>` when a query filters the view
  empty. The default `Off` leaves the classic type-to-search lookup unchanged.
```

- [ ] **Step 8: Run the full gate** (all three commands). Expected: PASS.

- [ ] **Step 9: Commit**

```bash
git add src/widgets/list_box.rs CHANGELOG.md
git commit -m "feat(list): SortedListBox self-filter; CHANGELOG"
```

---

## Self-Review notes (verify before execution)

- **Spec coverage:** find mode core (Tasks 1, 3); `query` accumulation + Backspace + Esc (Task 3); change broadcast (Tasks 1, 3); `find_query()` (Task 1); `clear_find` (Task 1); self-filter switch (Tasks 5, 6); highlight split (Task 4); `No match: <query>` placeholder (Task 4); key routing incl. Space-as-query and Enter/arrows unchanged (Task 3); layering across shared core vs concrete (Tasks 1–4 vs 5–6); all four Testing bullets (Task 1 query/Task 3 broadcast-on-change-only/Task 2 highlight split/Tasks 5–6 self-filter/Task 4 empty state).
- **Deliberately deferred (spec Non-goals):** fuzzy/regex/multi-term matching; all-occurrences highlight; a dedicated accent palette entry (reuses `roles.selected` — documented in Task 4). `MatchKind` is not added.
- **Type consistency:** `FindMode` spelled identically everywhere; `find_match`/`filtered_view`/`apply_view_len` are `pub(crate)` and called via `list_viewer::…` from `list_box.rs`; the filter→range→focus logic lives once in `apply_view_len`/`filtered_view`, so both `rebuild_view`s are thin 3-line wrappers (no duplicated logic block); `on_query_changed`/`find_query`/`clear_find` match the Task 1 trait signatures.
- **Open item for the implementer:** confirm the exact line numbers before each edit (they are from the current HEAD `fe606b1`; a prior task's insertions shift later line numbers within the same file — anchor edits on the quoted surrounding code, not the bare line number).
```
