# Data-Movement Phase 2 — widen `FieldValue` (`Bits`) so clusters participate

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Add `FieldValue::Bits(u32)` and make the cluster controls (`CheckBoxes`/`RadioButtons`/`MultiCheckBoxes`) transfer their packed bit value through the `value`/`set_value` protocol, so a dialog's `gather_data`/`scatter_data` now includes them.

**Architecture:** Add one variant to `FieldValue`. Implement `value()`/`set_value()` once on the shared `Cluster` engine (returning/accepting `Bits`); the three wrapper widgets already embed-and-delegate to `Cluster`, so they pick the behavior up simply by removing `value`/`set_value` from their `#[delegate(skip(...))]` lists. `Group::gather_data` already walks `child.view.value()`, so clusters begin participating automatically. Rendering is unchanged; the only behavior change is that clusters now contribute to data transfer.

**Tech Stack:** Rust (`tvision-rs` workspace), `insta` snapshots, the `#[delegate]` proc-macro + `tests/delegate_view.rs` spy test, mdBook guide, `cargo xtask test`/`docs`.

**Spec:** `docs/superpowers/specs/2026-06-18-unified-data-movement-design.md` — §3.1 (widen `FieldValue`), §5 "Phase 2", §9 docs.

## Global Constraints

- **Build env:** `export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target` before every cargo command (artifacts land there, not `./target`).
- **Parallelism cap:** never exceed 4 cores — `-j2` and `-- --test-threads=2` on every cargo invocation.
- **Faithful to C++**; English identifiers/comments. Commit messages end with `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`.
- **Additive, rendering-preserving:** no snapshot changes. The intended behavior change is that cluster controls now contribute to `gather_data`/`scatter_data` (previously `None`). Any existing test that asserted a cluster gathers `None` is updated to the new `Some(Bits(..))` (the change is the point); update only genuine behavior-change assertions, never weaken an unrelated one.
- **`#[delegate]` discipline (house rule):** un-skipping `value`/`set_value` on a wrapper makes the macro forward them to `self.cluster.value()`/`set_value()`. The `value`/`set_value`/`set_value_ctx` forwarders already exist in `tvision-rs-macros/src/specs.rs` (lines 40–45) — **no macro change needed.** Run `tests/delegate_view.rs`; if it carries a per-type expected-skip list, update it to reflect that these three now forward `value`/`set_value`.

### Deliberate scope decisions (recorded per the spec's §2.1 judgment guard — "apply where it helps, not where it burdens")

This phase adds **only** `Bits`. The spec's Phase-2 line also named `Bool` and `List`; both are deliberately **not** added here, each with its reason (the house "ported-or-deliberately-not-with-reason" rule):

- **`Bool` — not added (no consumer, now or across the 5 phases).** No control transfers a bare bool: check boxes and radio buttons are packed bit clusters (`Bits`), input lines are `Text`, scroll bars are `Int`. A standalone `Bool` variant would be dead vocabulary. If a future single-toggle control ever wants it, adding it then is a one-line change.
- **`List(Vec<FieldValue>)` — deferred to Phase 4 (where its consumers land).** `List` is the "a whole record/dialog as one `FieldValue`" image. Its real consumers are the view-launched modal record delivery and the Find/Replace modal reads — all Phase 4. Adding it now would be speculative, **and** it carries an unresolved design question best settled alongside its consumer: `Group::gather_data` returns `Vec<Option<FieldValue>>` (positional, with `None` gaps for non-data children), which a flat `List(Vec<FieldValue>)` cannot represent without losing positional alignment. Phase 4 settles `List`'s exact shape with the consumer in view.

---

### Task 1: Add `FieldValue::Bits(u32)`

**Files:**
- Modify: `src/data.rs` — the `FieldValue` enum, the module doc (lines ~14–20), and the enum doc (lines ~31–35). Add a test in the existing `#[cfg(test)] mod tests`.

**Interfaces:**
- Consumes: nothing new.
- Produces: `FieldValue::Bits(u32)` — a new public variant on the existing `pub enum FieldValue`.

- [ ] **Step 1: Write the failing test**

Add to `src/data.rs`'s `#[cfg(test)] mod tests`:

```rust
    #[test]
    fn bits_variant_round_trips() {
        let v = FieldValue::Bits(0b101);
        assert_eq!(v, FieldValue::Bits(0b101));
        assert_ne!(v, FieldValue::Bits(0b100));
        // Distinct from Int even when the bit pattern equals the integer.
        assert_ne!(FieldValue::Bits(0), FieldValue::Int(0));
        let FieldValue::Bits(b) = v else {
            panic!("expected Bits");
        };
        assert_eq!(b, 0b101);
    }
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test -p tvision-rs --lib -j2 -- --test-threads=2 bits_variant_round_trips
```
Expected: FAIL to compile — `no variant ... named Bits found for enum FieldValue`.

- [ ] **Step 3: Add the variant**

In `pub enum FieldValue`, after the `Int(i32)` variant, add:

```rust
    /// A cluster control's packed bit word — check boxes as a checked-bit mask,
    /// radio buttons as the selected index in bit position, multi-state boxes as
    /// the n-bits-per-item packing. Read/written via
    /// [`View::value`](crate::view::View::value)/[`set_value`](crate::view::View::set_value).
    Bits(u32),
```

- [ ] **Step 4: Correct the module + enum docs (they currently say clusters don't participate)**

In the module doc, replace the sentence block that begins "Cluster controls (check boxes, radio buttons) interpret their packed bit value internally and do not participate in dialog data transfer, so there is no `Bits` variant; the color picker likewise reports its color through a dedicated accessor rather than a `FieldValue`." with:

```rust
//! [`Bits`](FieldValue::Bits) for cluster controls (check boxes, radio buttons),
//! which transfer their packed bit value. The color picker is the one control
//! that reports its result through a dedicated accessor (a `Color`) rather than a
//! `FieldValue` — `Color` is a 4-variant enum, not a packable scalar.
```

In the enum doc comment, replace "Carries one variant per kind of value a control transfers. Cluster controls (check boxes, radio buttons) keep their bit value internal and the color picker uses a dedicated accessor, so neither has a variant here." with:

```rust
/// Carries one variant per kind of value a control transfers. The color picker
/// is the one control that uses a dedicated accessor (its result is a `Color`, a
/// 4-variant enum, not a packable scalar), so it has no variant here.
```

- [ ] **Step 5: Run the test + the data.rs suite**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test -p tvision-rs --lib -j2 -- --test-threads=2 data::
```
Expected: `bits_variant_round_trips`, `text_variant_round_trips`, `int_variant_round_trips` all PASS.

- [ ] **Step 6: Commit**

```bash
git add src/data.rs
git commit -m "feat(data): add FieldValue::Bits for cluster controls

Widen the data currency with Bits(u32) — the packed bit word check boxes / radio
buttons transfer. Correct the module/enum docs that claimed clusters do not
participate (Phase 2 makes them participate). No consumer yet; Task 2 wires the
cluster widgets.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2: Cluster controls transfer `Bits` via `value`/`set_value`

**Files:**
- Modify: `src/widgets/cluster.rs` — add `value`/`set_value` to `impl View for Cluster` (the base impl beginning at the `impl View for Cluster {` line); remove `value, set_value` from the `#[delegate(... skip(...))]` lists on `CheckBoxes`, `RadioButtons`, `MultiCheckBoxes`. Add tests in its `#[cfg(test)] mod`.
- Possibly modify: `tests/delegate_view.rs` — only if it carries a per-type expected-skip list that must now show `value`/`set_value` forwarding.
- Verify (likely no change): `src/view/group.rs` gather/scatter tests (the existing ones use `InputLine`, not clusters, so they should be unaffected — confirm).

**Interfaces:**
- Consumes: `FieldValue::Bits` (Task 1); `Cluster::value: u32` (the packed bit word, `cluster.rs:128`); the `#[delegate]` macro's existing `value`/`set_value` forwarders (`tvision-rs-macros/src/specs.rs:40-45`).
- Produces: `Cluster::value()` returns `Some(FieldValue::Bits(self.value))`; `Cluster::set_value(FieldValue::Bits(b))` sets `self.value = b`. `CheckBoxes`/`RadioButtons`/`MultiCheckBoxes` forward both to `cluster` via the macro. `Group::gather_data` now yields `Some(Bits(..))` for a cluster child (it already calls `child.view.value()`).

- [ ] **Step 1: Write the failing test**

Add to `src/widgets/cluster.rs`'s `#[cfg(test)] mod` (it already has helpers building `CheckBoxes`/`RadioButtons`; mirror their style). These two tests assert the new transfer behavior on the public wrappers:

```rust
    #[test]
    fn checkboxes_transfer_bits_via_value_protocol() {
        use crate::data::FieldValue;
        use crate::view::View;

        let mut cb = CheckBoxes::new(Rect::new(0, 0, 20, 4), vec!["a".into(), "b".into(), "c".into()]);
        cb.cluster.value = 0b101;
        assert_eq!(cb.value(), Some(FieldValue::Bits(0b101)), "value() reports the packed bit word");

        cb.set_value(FieldValue::Bits(0b010));
        assert_eq!(cb.cluster.value, 0b010, "set_value(Bits) writes the packed bit word");

        // A non-Bits value is ignored (clusters only accept Bits).
        cb.set_value(FieldValue::Int(7));
        assert_eq!(cb.cluster.value, 0b010, "set_value ignores non-Bits kinds");
    }

    #[test]
    fn radiobuttons_transfer_bits_via_value_protocol() {
        use crate::data::FieldValue;
        use crate::view::View;

        let mut rb = RadioButtons::new(Rect::new(0, 0, 20, 4), vec!["x".into(), "y".into()]);
        rb.cluster.value = 1;
        assert_eq!(rb.value(), Some(FieldValue::Bits(1)), "radio value() reports the selected index as Bits");
        rb.set_value(FieldValue::Bits(0));
        assert_eq!(rb.cluster.value, 0, "radio set_value(Bits) writes the selection");
    }
```

- [ ] **Step 2: Run the test to verify it fails**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test -p tvision-rs --lib -j2 -- --test-threads=2 transfer_bits_via_value_protocol
```
Expected: FAIL — `CheckBoxes::value()` currently returns `None` (the wrappers skip `value`/`set_value`, so the `View` default applies): the first `assert_eq!` fails (`None != Some(Bits(..))`).

- [ ] **Step 3: Implement `value`/`set_value` on `Cluster`**

In `impl View for Cluster { … }` (the base impl), add these two methods (place them near the other small accessor methods; order within the impl is not significant):

```rust
    /// A cluster transfers its packed bit value as [`FieldValue::Bits`].
    fn value(&self) -> Option<crate::data::FieldValue> {
        Some(crate::data::FieldValue::Bits(self.value))
    }

    /// Accept a [`FieldValue::Bits`] into the packed bit value; other kinds are
    /// ignored (a cluster only transfers a bit word).
    fn set_value(&mut self, v: crate::data::FieldValue) {
        if let crate::data::FieldValue::Bits(bits) = v {
            self.value = bits;
        }
    }
```

- [ ] **Step 4: Un-skip `value`/`set_value` on the three wrappers so the macro forwards them**

Change the three `#[delegate(...)]` attribute lines, removing `set_value, value` from each `skip(...)` list (leave every other skipped method exactly as-is):

`CheckBoxes` (currently): `#[crate::delegate(to = cluster, skip(apply_list_scroll, focus_descendant, grabs_focus_on_click, set_value, value))]`
→ `#[crate::delegate(to = cluster, skip(apply_list_scroll, focus_descendant, grabs_focus_on_click))]`

`RadioButtons` (currently): `#[crate::delegate(to = cluster, skip(apply_list_scroll, as_any_mut, focus_descendant, grabs_focus_on_click, set_value, value))]`
→ `#[crate::delegate(to = cluster, skip(apply_list_scroll, as_any_mut, focus_descendant, grabs_focus_on_click))]`

`MultiCheckBoxes` (currently): `#[crate::delegate(to = cluster, skip(apply_list_scroll, as_any_mut, focus_descendant, grabs_focus_on_click, set_value, value))]`
→ `#[crate::delegate(to = cluster, skip(apply_list_scroll, as_any_mut, focus_descendant, grabs_focus_on_click))]`

(Leave `CheckBoxes`'s manual `as_any_mut` method as-is — it is NOT in `CheckBoxes`'s skip list because the manual impl provides it; do not touch it. The Phase-4 work removes the `apply_modal_completion` downcast that relies on it.)

- [ ] **Step 5: Add the gather/scatter integration test**

Add to `src/widgets/cluster.rs`'s test module a test proving a cluster now participates in a group's data walk (mirror the helper style already used in `group.rs`'s gather tests — build a `Group`, insert a `CheckBoxes`, call `gather_data`/`scatter_data` with a throwaway `Context`):

```rust
    #[test]
    fn cluster_participates_in_group_gather_scatter() {
        use crate::data::FieldValue;
        use crate::event::Event;
        use crate::time::TimerQueue;
        use crate::view::{Context, Group};
        use std::collections::VecDeque;

        let mut group = Group::new(Rect::new(0, 0, 30, 6));
        let mut cb = CheckBoxes::new(Rect::new(1, 1, 20, 4), vec!["a".into(), "b".into()]);
        cb.cluster.value = 0b10;
        let cb_id = group.insert(Box::new(cb));

        // Gather: the cluster's slot now carries Bits (previously None).
        let gathered = group.gather_data();
        let idx = group.gather_data().len() - gathered.len(); // 0; explicit for clarity
        let _ = idx;
        assert!(
            gathered.iter().any(|v| *v == Some(FieldValue::Bits(0b10))),
            "gather_data includes the cluster's Bits value"
        );

        // Scatter a new value back through the context-aware setter.
        let mut out: VecDeque<Event> = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred = VecDeque::new();
        let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
        group.scatter_data(&[Some(FieldValue::Bits(0b01))], &mut ctx);
        let after = group
            .child_mut(cb_id)
            .and_then(|v| v.value());
        assert_eq!(after, Some(FieldValue::Bits(0b01)), "scatter_data writes the cluster value back");
    }
```

NOTE: the exact `Context::new` argument list and `Group`/`TimerQueue` import paths must match the project's current signatures — check the existing gather/scatter tests in `src/view/group.rs` (around the `scatter_data_round_trips_with_gather` test) and mirror them precisely; adjust the snippet's imports/`Context::new` call to match if they differ.

- [ ] **Step 6: Run the new tests + the cluster suite + the delegate spy + gather/scatter**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test -p tvision-rs --lib -j2 -- --test-threads=2 transfer_bits_via_value_protocol cluster_participates_in_group_gather_scatter
cargo test -p tvision-rs --lib -j2 -- --test-threads=2 cluster::
cargo test -p tvision-rs --test delegate_view -j2 -- --test-threads=2
cargo test -p tvision-rs --lib -j2 -- --test-threads=2 gather_data scatter_data
```
Expected: the new tests PASS; the cluster suite stays green; the `delegate_view` spy passes (it confirms `value`/`set_value` now forward on the three wrappers — if it carries a per-type expected-skip list, update that list so it reflects the un-skip, then re-run); the existing `group.rs` gather/scatter tests stay green (they use `InputLine`, not clusters).

- [ ] **Step 7: Build gate + commit**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo clippy -p tvision-rs --all-targets -j2 -- -D warnings
git add -A
git commit -m "feat(cluster): cluster controls transfer Bits via value/set_value

Implement value()/set_value() on the shared Cluster engine (Bits(self.value)),
and un-skip value/set_value on CheckBoxes/RadioButtons/MultiCheckBoxes so the
delegate macro forwards them. Group::gather_data/scatter_data now include cluster
controls (previously None). CheckBoxes::as_any_mut stays (Phase 4 removes the
FindPick/ReplacePick downcast that uses it).

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 3: Documentation (lands WITH the phase, per spec §9)

**Files:**
- Modify: `docs/book/src/apps/dialogs.md` — note that cluster controls now participate in `gather`/`scatter` via `FieldValue::Bits`.

**Interfaces:** none (documentation only).

- [ ] **Step 1: Add the guide note**

In `docs/book/src/apps/dialogs.md`, find the section that describes the `FieldValue` data currency / `gather`/`scatter` (the "Dialogs & data" content). Add a short paragraph there:

```markdown
Cluster controls — check boxes and radio buttons — transfer their state as a
packed bit word: [`FieldValue::Bits`](../api/tvision-rs/data/enum.FieldValue.html).
A check-box cluster reports the set of checked boxes as a bitmask; a radio group
reports the selected index in bit position. Because clusters participate in
`value`/`set_value`, a dialog's `gather_data`/`scatter_data` round-trips them like
any other field — no special-casing in consumer code. (The color picker is the
one control that stays off `FieldValue`: its result is a `Color`, a 4-variant
enum, returned by value via `exec_view_with`.)
```

If the page has no obvious data-currency section, append it under the most relevant existing heading rather than inventing a new top-level chapter.

- [ ] **Step 2: Run the doc gates**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo xtask test    # guide rust blocks compile
cargo xtask docs    # regenerate + build the integrated site
```
Expected: `cargo xtask test` succeeds. NOTE: `cargo xtask docs` link-check is a **pre-existing failure** on this repo (~720 broken `../api/` links, present on `main` too — the local checker cannot resolve rustdoc `api/` links). The new `enum.FieldValue.html` link follows the same pattern as the existing broken `../api/` links. Confirm the failure count/pattern matches the pre-existing one (i.e. your change did not introduce a NEW class of breakage); if `cargo xtask docs` regenerated any `docs/book/src/screens/*.html` screenshots as a side effect, do NOT commit them — `git checkout -- docs/book/src/screens/` to discard that regeneration noise, leaving only your `dialogs.md` edit staged.

- [ ] **Step 3: Commit**

```bash
git add docs/book/src/apps/dialogs.md
git commit -m "docs(guide): note cluster controls transfer FieldValue::Bits

Document in apps/dialogs.md that check boxes / radio buttons now round-trip
through gather/scatter as FieldValue::Bits, per the Phase 2 widening.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Final verification (whole-phase, before the broad review)

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test --workspace -j2 -- --test-threads=2
cargo clippy --workspace --all-targets -j2 -- -D warnings
cargo fmt --all --check
```
Expected: green; no snapshot changes (rendering is unchanged — only data transfer gained the cluster values).

## Self-Review notes (author)

- **Spec coverage (Phase 2):** `Bits` added (Task 1) ✓; cluster `value()`/`set_value()` honest (Task 2) ✓; `gather_data`/`scatter_data` now include clusters (Task 2, automatic via the existing walk) ✓; docs with the phase (Task 3) ✓. **Deliberately not done, with recorded reasons:** `Bool` (no consumer) and `List` (deferred to Phase 4 with its consumers + the `Vec<Option>`-vs-`List` design question) — see Global Constraints.
- **Type consistency:** `value()` returns `Option<FieldValue>`, `set_value(FieldValue)` — matching the existing `View` trait signatures and the macro forwarders (`specs.rs:40-45`). `Cluster::value` is `u32`, packed straight into `Bits(u32)`.
- **No macro/spy surprise:** `value`/`set_value`/`set_value_ctx` forwarders already exist in `specs.rs`; un-skipping enables them. The `delegate_view` spy is run in Task 2 and its expected-skip list updated if present.
- **`Color` boundary kept:** the docs explicitly keep the color picker off `FieldValue` (Phase 1 C-1), so Phase 2 does not accidentally re-open that question.
