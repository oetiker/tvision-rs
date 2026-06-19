# Data-Movement Phase 2 — widen `FieldValue` (+ the `Custom` open seam) Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make `FieldValue` the single typed data currency for the framework's controls *and* third-party components — widen it (`Bool`/`Bits`/`List`/`Custom`), give clusters honest `value()`/`set_value()`, expose a group's whole record as an ordered `List`, and add the runtime-checked-but-fail-loud `Custom` extensibility seam.

**Architecture:** `Custom(Rc<dyn CustomValue>)` is the open escape for user-invented payloads (`CustomValue: Any + Debug`, blanket-impl'd so authors write nothing). The framework moves it opaquely; consumers downcast at the edge via the loud `value_as::<T>() -> Result`. Cluster controls report their `u32` bit word as `Bits`. `Group` gains `gather_list`/`scatter_list` (the ordered-`List` record image of C++ `getData(void*)`) built on the existing positional `gather_data`/`scatter_data` primitive, which is left unchanged.

**Tech Stack:** Rust (`tvision-rs` workspace crate), `insta` snapshots, mdBook guide (`docs/book/`), `cargo xtask test`/`docs`. **No new external dependency.**

**Spec:** `docs/superpowers/specs/2026-06-18-unified-data-movement-design.md` — §3.1 (widen `FieldValue`, `Color` is NOT a `FieldValue`), §3.5 (extensibility: `Custom`, `value_as`, runtime-checked/fail-safe), §5 Phase 2, §9 docs.

## Scope decisions (deliberate; flagged for review)

1. **The positional primitive stays.** `gather_data() -> Vec<Option<FieldValue>>` and `scatter_data(&[Option<FieldValue>])` are unchanged (their `None`-skips positional alignment is needed and their tests must stay green). The spec's "gather/scatter produce/consume an ordered `List`" is delivered as **new** explicit methods `Group::gather_list`/`scatter_list` (Task 3) — the whole-dialog-as-one-`FieldValue::List`. This is behavior-preserving and avoids a `View::value` override on `Group` that would silently change nested-group gathering.
2. **`self_check` is deferred to a follow-on.** The spec's optional `inventory`-collected `Program::self_check()` + `data_self_check` convention (§3.5) needs a new external crate (`inventory`). That is a dependency decision for the owner, out of scope here. This plan establishes the dependency-free core (the `Custom`/`value_as` seam) on which a later self-test layer can build. **Recorded as a follow-on, not dropped.**
3. **`Color`/`ColorPicker` are untouched.** `Color` rides the by-value path (Phase 1), not `FieldValue` (spec C-1). No `value()` on `ColorPicker`.

## Global Constraints

- **Build env:** `export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target` before every cargo command.
- **Parallelism cap:** never exceed 4 cores — `-j2` and `-- --test-threads=2` on every cargo invocation.
- **Faithful to C++**; English identifiers/comments. Commit messages end with `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`.
- **Behavior-preserving:** no snapshot may change; existing tests keep their asserted behavior. New capability is additive (new variants, new methods, new cluster impls that nothing consumes implicitly except the already-tested-with-`InputLine` `gather_data`).
- **`Color` is NOT a `FieldValue`** (C-1): no `Custom`/`Bits` packing of `Color`; `ColorPicker` gets no `value()`.
- **`Custom` is the user round-tripping their own type** through an agnostic framework (correct `std::any` use, not the framework-internal downcast smell removed elsewhere). Access is runtime-checked and **fail-loud** via `value_as` (spec §3.5).
- **No new public item without rustdoc**, house convention (user-facing; C++ lineage in a `# Turbo Vision heritage` section).
- **No new external dependency** in this plan (see Scope decision 2).

---

### Task 1: Widen `FieldValue` + the `Custom` seam (accessors, `FieldTypeError`)

**Files:**
- Modify: `src/data.rs` — the `FieldValue` enum, its derives, the module doc (lines 1–29) and enum doc (lines 31–35); add `CustomValue`, `FieldTypeError`, the accessor `impl`, and a manual `PartialEq`.
- Test: `src/data.rs` `#[cfg(test)] mod tests` (extend).

**Interfaces:**
- Consumes: `std::any::Any`, `std::rc::Rc`, `std::fmt`.
- Produces:
  - `enum FieldValue { Text(String), Int(i32), Bool(bool), Bits(u32), List(Vec<FieldValue>), Custom(Rc<dyn CustomValue>) }` (derives `Clone, Debug`; manual `PartialEq`).
  - `pub trait CustomValue: Any + Debug { fn as_any_rc(self: Rc<Self>) -> Rc<dyn Any>; }` with blanket `impl<T: Any + Debug> CustomValue for T`.
  - `impl FieldValue { pub fn custom<T: Any + Debug>(v: T) -> Self; pub fn as_custom<T: Any>(&self) -> Option<Rc<T>>; pub fn value_as<T: Any>(&self) -> Result<Rc<T>, FieldTypeError>; }`
  - `pub struct FieldTypeError { pub expected: &'static str, pub found: &'static str }` (`Debug, Clone, PartialEq`; `Display`; `std::error::Error`).

- [ ] **Step 1: Write the failing tests**

Add to `src/data.rs`'s `mod tests` (the existing `text_variant_round_trips`/`int_variant_round_trips` stay):

```rust
    #[test]
    fn new_scalar_variants_round_trip() {
        assert_eq!(FieldValue::Bool(true), FieldValue::Bool(true));
        assert_ne!(FieldValue::Bool(true), FieldValue::Bool(false));
        assert_eq!(FieldValue::Bits(0b101), FieldValue::Bits(0b101));
        assert_eq!(
            FieldValue::List(vec![FieldValue::Int(1), FieldValue::Text("a".into())]),
            FieldValue::List(vec![FieldValue::Int(1), FieldValue::Text("a".into())]),
        );
        // Distinct kinds never compare equal.
        assert_ne!(FieldValue::Bool(true), FieldValue::Int(1));
        assert_ne!(FieldValue::Bits(0), FieldValue::Int(0));
    }

    #[derive(Debug, PartialEq)]
    struct DateRange { start: i32, end: i32 }

    #[test]
    fn custom_round_trips_via_as_custom() {
        let fv = FieldValue::custom(DateRange { start: 1, end: 9 });
        let got = fv.as_custom::<DateRange>().expect("downcast to the stored type");
        assert_eq!(*got, DateRange { start: 1, end: 9 });
        // Wrong type → None (fail closed).
        assert!(fv.as_custom::<String>().is_none());
    }

    #[test]
    fn value_as_is_loud_on_mismatch() {
        let fv = FieldValue::custom(DateRange { start: 1, end: 9 });
        assert!(fv.value_as::<DateRange>().is_ok(), "matching type succeeds");

        // Wrong Custom type → descriptive error, not None.
        let err = fv.value_as::<String>().unwrap_err();
        assert!(err.expected.contains("String"), "names the expected type: {err}");

        // A scalar read as a Custom → error naming the found variant.
        let scalar = FieldValue::Int(3);
        let err = scalar.value_as::<DateRange>().unwrap_err();
        assert_eq!(err.found, "Int", "names the found variant");
    }

    #[test]
    fn custom_equality_is_pointer_identity() {
        let a = FieldValue::custom(DateRange { start: 1, end: 2 });
        let b = a.clone(); // Rc clone — same allocation
        let c = FieldValue::custom(DateRange { start: 1, end: 2 }); // distinct allocation
        assert_eq!(a, b, "clones share the Rc, so they are equal by identity");
        assert_ne!(a, c, "distinct allocations are not equal even with equal contents");
    }
```

- [ ] **Step 2: Run the tests to verify they fail**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test -p tvision-rs --lib -j2 -- --test-threads=2 data::tests
```
Expected: FAIL to compile — `FieldValue::Bool`/`Bits`/`List`/`custom`/`as_custom`/`value_as` and `FieldTypeError` do not exist.

- [ ] **Step 3: Widen the enum + add the trait, error, accessors, manual PartialEq**

Replace the imports/enum/derives region of `src/data.rs` (the `#[derive(...)] pub enum FieldValue { Text, Int }` block at lines 31–43) with:

```rust
use std::any::Any;
use std::fmt;
use std::rc::Rc;

/// Marker for a user-invented payload carried in [`FieldValue::Custom`].
///
/// Blanket-implemented for every `'static` type that is [`Debug`], so component
/// authors implement nothing — they just put their type in a `FieldValue::custom`.
/// `Debug` is required so a payload is inspectable (and so [`FieldValue`] keeps a
/// derived `Debug`). The `as_any_rc` bridge lets the typed accessors downcast.
pub trait CustomValue: Any + fmt::Debug {
    /// Upcast `Rc<Self>` to `Rc<dyn Any>` so [`FieldValue::as_custom`] /
    /// [`FieldValue::value_as`] can `downcast`.
    fn as_any_rc(self: Rc<Self>) -> Rc<dyn Any>;
}

impl<T: Any + fmt::Debug> CustomValue for T {
    fn as_any_rc(self: Rc<Self>) -> Rc<dyn Any> {
        self
    }
}

/// The typed unit of dialog data transfer (the D10 value currency).
///
/// Well-known shapes ([`Text`](FieldValue::Text)/[`Int`](FieldValue::Int)/
/// [`Bool`](FieldValue::Bool)/[`Bits`](FieldValue::Bits)/[`List`](FieldValue::List))
/// are fully type-checked and interoperate with framework controls and generic
/// consumers. [`Custom`](FieldValue::Custom) is the open seam for user-invented
/// payloads: the framework moves it opaquely and a consumer downcasts at the edge
/// (runtime-checked; see [`value_as`](FieldValue::value_as)).
#[derive(Clone, Debug)]
pub enum FieldValue {
    /// A text field's contents (an input line).
    Text(String),
    /// An integer value (e.g. a scroll bar's position).
    Int(i32),
    /// A boolean field.
    Bool(bool),
    /// A packed bit word — a cluster control's value (check boxes: a bitmask;
    /// radio buttons: the selected index). Faithful to `TCluster::value`. NOT a
    /// packed `Color` (`Color` is a 4-variant enum and rides the by-value path).
    Bits(u32),
    /// An ordered record — the typed image of C++ `getData(void *rec)`'s
    /// offset-addressed child walk (positional, anonymous). See
    /// [`Group::gather_list`](crate::view::Group::gather_list).
    List(Vec<FieldValue>),
    /// A user-invented payload, carried opaquely. Construct with
    /// [`custom`](FieldValue::custom); read with [`value_as`](FieldValue::value_as)
    /// (loud) or [`as_custom`](FieldValue::as_custom) (`Option`). Equality is
    /// **pointer identity** (two `Custom`s are equal iff they share the `Rc`).
    Custom(Rc<dyn CustomValue>),
}

impl FieldValue {
    /// Wrap a user payload as [`Custom`](FieldValue::Custom).
    pub fn custom<T: Any + fmt::Debug>(v: T) -> Self {
        FieldValue::Custom(Rc::new(v))
    }

    /// Read a [`Custom`](FieldValue::Custom) payload as `T`, or `None` if this is
    /// not a `Custom` of type `T` (fail closed). For a descriptive error instead,
    /// use [`value_as`](Self::value_as).
    pub fn as_custom<T: Any>(&self) -> Option<Rc<T>> {
        match self {
            FieldValue::Custom(rc) => rc.clone().as_any_rc().downcast::<T>().ok(),
            _ => None,
        }
    }

    /// Read a [`Custom`](FieldValue::Custom) payload as `T`, **loudly**: a type
    /// mismatch returns a descriptive [`FieldTypeError`] (so stale producer/
    /// consumer wiring announces itself at first execution) rather than a silent
    /// `None`. This is the recommended accessor for third-party components.
    pub fn value_as<T: Any>(&self) -> Result<Rc<T>, FieldTypeError> {
        match self {
            FieldValue::Custom(rc) => rc.clone().as_any_rc().downcast::<T>().map_err(|_| {
                FieldTypeError {
                    expected: std::any::type_name::<T>(),
                    found: "a different Custom payload type",
                }
            }),
            other => Err(FieldTypeError {
                expected: std::any::type_name::<T>(),
                found: other.variant_name(),
            }),
        }
    }

    /// The variant name, for diagnostics.
    fn variant_name(&self) -> &'static str {
        match self {
            FieldValue::Text(_) => "Text",
            FieldValue::Int(_) => "Int",
            FieldValue::Bool(_) => "Bool",
            FieldValue::Bits(_) => "Bits",
            FieldValue::List(_) => "List",
            FieldValue::Custom(_) => "Custom",
        }
    }
}

impl PartialEq for FieldValue {
    /// Scalars and `List` compare by value; `Custom` compares by **pointer
    /// identity** (`Rc::ptr_eq`) — the framework cannot compare opaque user
    /// payloads by value, so two `Custom`s are equal iff they share the `Rc`.
    fn eq(&self, other: &Self) -> bool {
        use FieldValue::*;
        match (self, other) {
            (Text(a), Text(b)) => a == b,
            (Int(a), Int(b)) => a == b,
            (Bool(a), Bool(b)) => a == b,
            (Bits(a), Bits(b)) => a == b,
            (List(a), List(b)) => a == b,
            (Custom(a), Custom(b)) => Rc::ptr_eq(a, b),
            _ => false,
        }
    }
}

/// A [`FieldValue::value_as`] type mismatch — names the expected type and the
/// found variant/payload, so a contract mismatch fails loudly.
#[derive(Debug, Clone, PartialEq)]
pub struct FieldTypeError {
    /// The Rust type the caller asked for (`std::any::type_name`).
    pub expected: &'static str,
    /// What was actually present (a variant name, or a different `Custom` type).
    pub found: &'static str,
}

impl fmt::Display for FieldTypeError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "FieldValue type mismatch: expected {}, found {}",
            self.expected, self.found
        )
    }
}

impl std::error::Error for FieldTypeError {}
```

- [ ] **Step 4: Rewrite the module doc to match v5**

Replace the module doc (the `//!` block, lines 1–29) so it no longer states the old "no `Bits` / clusters don't participate" decision:

```rust
//! Typed dialog-data transfer — the value currency moved between controls and
//! the dialog that owns them, and the open seam for third-party components.
//!
//! A control exposes its current value as a [`FieldValue`] via the
//! [`value`](crate::view::View::value)/[`set_value`](crate::view::View::set_value)
//! pair on the [`View`](crate::view::View) trait. A dialog gathers the whole
//! record by walking its children in order
//! ([`Group::gather_data`](crate::view::Group::gather_data) →
//! `Vec<Option<FieldValue>>`, the positional primitive;
//! [`Group::gather_list`](crate::view::Group::gather_list) → one ordered
//! [`FieldValue::List`]) and distributes edited values back the same way
//! ([`Group::scatter_data`](crate::view::Group::scatter_data) /
//! [`Group::scatter_list`](crate::view::Group::scatter_list)).
//!
//! [`FieldValue`] carries the well-known shapes a control transfers
//! ([`Text`](FieldValue::Text), [`Int`](FieldValue::Int),
//! [`Bool`](FieldValue::Bool), [`Bits`](FieldValue::Bits) for cluster controls,
//! [`List`](FieldValue::List) for a whole record) plus
//! [`Custom`](FieldValue::Custom) — the open escape for payloads a user-written
//! component invents. `Color` is deliberately NOT a `FieldValue` (it is a
//! 4-variant enum and rides the by-value `exec_view_with` path).
//!
//! **Extensibility:** see the [extensibility guide](../../../apps/extensibility.html)
//! for the three open paths and the `Custom` / [`value_as`](FieldValue::value_as)
//! contract (runtime-checked, fail-loud, typed at the edges).
//!
//! **Guide:** [Dialogs & data](../../../apps/dialogs.html).
//!
//! # Turbo Vision heritage
//!
//! The original moved dialog data through an untyped getter/setter protocol over a
//! raw record (`getData`/`setData`/`dataSize`, anonymous `void*`). tvision-rs replaces
//! that with this typed value currency (deviation D10); the `Custom` seam keeps the
//! original's openness to arbitrary payloads without its loss of type safety.
```

- [ ] **Step 5: Run the new tests + the full lib suite**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test -p tvision-rs --lib -j2 -- --test-threads=2 data::tests
cargo test -p tvision-rs --lib -j2 -- --test-threads=2
cargo clippy -p tvision-rs --all-targets -j2 -- -D warnings
```
Expected: the new `data::tests` PASS; the full lib suite stays green (the manual `PartialEq` must not regress any `FieldValue` comparison elsewhere); clippy clean.

- [ ] **Step 6: Commit**

```bash
git add src/data.rs
git commit -m "feat(data): widen FieldValue (Bool/Bits/List) + Custom open seam

Add the well-known scalar shapes and FieldValue::Custom(Rc<dyn CustomValue>) — the
open, typed-at-the-edges escape for user-invented payloads (CustomValue: Any+Debug,
blanket-impl'd). Accessors: custom(), as_custom() (Option), value_as() (loud
Result via FieldTypeError). Custom equality is pointer identity; manual PartialEq
(dyn payloads aren't comparable by value). Color stays by-value, not a FieldValue.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 2: Cluster controls report their value (`Bits`)

**Files:**
- Modify: `src/widgets/cluster.rs` — the `impl View for CheckBoxes` block (the `#[delegate(... skip(... set_value, value))]` block at ~738–745) and `impl View for RadioButtons` (the `skip(... set_value, value)` block at ~759–760).
- Test: `src/widgets/cluster.rs` `#[cfg(test)] mod tests` (extend).

**Interfaces:**
- Consumes: `Cluster.value: u32` (the bit word, `cluster.rs:128`); `FieldValue::Bits` (Task 1).
- Produces: `value()`/`set_value()` on `CheckBoxes` and `RadioButtons`. The delegate `skip(set_value, value)` lists already exclude these from forwarding, so adding explicit impls needs **no** macro/`specs.rs` change.

- [ ] **Step 1: Write the failing tests**

Add to `cluster.rs`'s `mod tests`:

```rust
    #[test]
    fn checkboxes_value_round_trips_as_bits() {
        use crate::data::FieldValue;
        use crate::view::View;
        let mut c = CheckBoxes::new(Rect::new(0, 0, 20, 4), &["a", "b", "c"]);
        c.cluster.value = 0b101;
        assert_eq!(c.value(), Some(FieldValue::Bits(0b101)));
        c.set_value(FieldValue::Bits(0b010));
        assert_eq!(c.cluster.value, 0b010, "set_value(Bits) writes the bit word");
        // A variant the control does not understand is ignored.
        c.set_value(FieldValue::Text("x".into()));
        assert_eq!(c.cluster.value, 0b010, "non-Bits value is ignored");
    }

    #[test]
    fn radiobuttons_value_round_trips_as_bits() {
        use crate::data::FieldValue;
        use crate::view::View;
        let mut r = RadioButtons::new(Rect::new(0, 0, 20, 4), &["a", "b", "c"]);
        r.cluster.value = 2; // selected index
        assert_eq!(r.value(), Some(FieldValue::Bits(2)));
        r.set_value(FieldValue::Bits(1));
        assert_eq!(r.cluster.value, 1, "set_value(Bits) sets the selected index");
    }
```

(If `CheckBoxes::new`/`RadioButtons::new` signatures differ, match the constructors the other tests in this file already use — read a nearby test in `mod tests` first.)

- [ ] **Step 2: Run to verify failure**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test -p tvision-rs --lib -j2 -- --test-threads=2 cluster::tests::checkboxes_value cluster::tests::radiobuttons_value
```
Expected: FAIL — `value()` returns `None` (trait default), assertions fail.

- [ ] **Step 3: Implement `value`/`set_value` on both clusters**

In `impl View for CheckBoxes` (add the two methods alongside the existing `as_any_mut`):

```rust
    /// This cluster's packed bit word as [`FieldValue::Bits`] (a bitmask). Ports
    /// `TCluster::getData` (copies `value`).
    fn value(&self) -> Option<crate::data::FieldValue> {
        Some(crate::data::FieldValue::Bits(self.cluster.value))
    }

    /// Load a [`FieldValue::Bits`] bit word; other variants are ignored. Ports
    /// `TCluster::setData`.
    fn set_value(&mut self, v: crate::data::FieldValue) {
        if let crate::data::FieldValue::Bits(bits) = v {
            self.cluster.value = bits;
        }
    }
```

In `impl View for RadioButtons {}` add the same two methods (for radio buttons the bit word is the selected index — same `u32`, same code):

```rust
    /// This cluster's value as [`FieldValue::Bits`] (the selected index). Ports
    /// `TCluster::getData`.
    fn value(&self) -> Option<crate::data::FieldValue> {
        Some(crate::data::FieldValue::Bits(self.cluster.value))
    }

    /// Load a [`FieldValue::Bits`] (the selected index); other variants ignored.
    fn set_value(&mut self, v: crate::data::FieldValue) {
        if let crate::data::FieldValue::Bits(bits) = v {
            self.cluster.value = bits;
        }
    }
```

(The `skip(... set_value, value)` in both delegate attributes already opt these out of forwarding, so the explicit impls are correct and need no macro change.)

- [ ] **Step 4: Run the new tests + cluster suite + delegate spy + clippy**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test -p tvision-rs --lib -j2 -- --test-threads=2 cluster::
cargo test --workspace -j2 -- --test-threads=2 delegate_view
cargo clippy -p tvision-rs --all-targets -j2 -- -D warnings
```
Expected: new cluster value tests PASS; the rest of `cluster::` unchanged; `delegate_view` spy PASS (no forwarder needed — `value`/`set_value` were already skipped); clippy clean.

- [ ] **Step 5: Commit**

```bash
git add src/widgets/cluster.rs
git commit -m "feat(cluster): CheckBoxes/RadioButtons report value as FieldValue::Bits

Clusters now participate in the typed data currency: value() -> Bits(cluster.value)
and set_value(Bits) writes it (TCluster::getData/setData). Both were already
skip()'d from delegate forwarding, so no macro/spy change. Additive — gather_data
(only consumed with InputLine today) now also surfaces cluster bits.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 3: A group's whole record as an ordered `List` (`gather_list`/`scatter_list`)

**Files:**
- Modify: `src/view/group.rs` — add two methods near `gather_data`/`scatter_data` (~218–250).
- Test: `src/view/group.rs` `#[cfg(test)] mod tests` (extend, near the existing gather/scatter tests ~2910).

**Interfaces:**
- Consumes: `Group::gather_data() -> Vec<Option<FieldValue>>`, `Group::scatter_data(&[Option<FieldValue>], &mut Context)` (unchanged primitives); `FieldValue::List` (Task 1).
- Produces:
  - `pub fn gather_list(&self) -> FieldValue` — returns `FieldValue::List` of the **data-bearing** children's values, in child order (children whose `value()` is `None` contribute nothing — the C++ `dataSize == 0` walk).
  - `pub fn scatter_list(&mut self, record: &FieldValue, ctx: &mut Context)` — accepts a `FieldValue::List` and writes its elements to the data-bearing children in order (skipping children whose `value()` is `None`); a non-`List` argument is ignored.

- [ ] **Step 1: Write the failing tests**

Add to `group.rs`'s `mod tests` (model the setup on `gather_data_returns_values_in_forward_child_order` ~2913 — reuse `with_ctx`, `InputLine`):

```rust
    #[test]
    fn gather_list_packs_data_bearing_children_in_order() {
        use crate::data::FieldValue;
        use crate::widgets::InputLine;

        let mut group = Group::new(Rect::new(0, 0, 40, 10));
        group.insert(Box::new(InputLine::new(
            Rect::new(0, 0, 10, 1), 20, None, crate::widgets::LimitMode::MaxBytes,
        )));
        // A non-data child (a bare Group) contributes nothing to the record.
        group.insert(Box::new(Group::new(Rect::new(0, 5, 5, 6))));
        group.insert(Box::new(InputLine::new(
            Rect::new(0, 2, 10, 3), 20, None, crate::widgets::LimitMode::MaxBytes,
        )));

        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        with_ctx(&mut out, &mut timers, |ctx| {
            group.scatter_list(
                &FieldValue::List(vec![
                    FieldValue::Text("alpha".into()),
                    FieldValue::Text("beta".into()),
                ]),
                ctx,
            );
        });

        // Two data-bearing children, in order; the bare Group is skipped.
        assert_eq!(
            group.gather_list(),
            FieldValue::List(vec![
                FieldValue::Text("alpha".into()),
                FieldValue::Text("beta".into()),
            ]),
        );
    }

    #[test]
    fn scatter_list_ignores_non_list() {
        use crate::data::FieldValue;
        use crate::widgets::InputLine;

        let mut group = Group::new(Rect::new(0, 0, 40, 10));
        group.insert(Box::new(InputLine::new(
            Rect::new(0, 0, 10, 1), 20, None, crate::widgets::LimitMode::MaxBytes,
        )));
        let before = group.gather_list();

        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        with_ctx(&mut out, &mut timers, |ctx| {
            group.scatter_list(&FieldValue::Int(7), ctx); // not a List → no-op
        });
        assert_eq!(group.gather_list(), before, "a non-List record changes nothing");
    }
```

- [ ] **Step 2: Run to verify failure**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test -p tvision-rs --lib -j2 -- --test-threads=2 group::tests::gather_list group::tests::scatter_list_ignores
```
Expected: FAIL to compile — `gather_list`/`scatter_list` do not exist.

- [ ] **Step 3: Implement the two methods**

Add to `impl Group` immediately after `scatter_data` (after line ~250):

```rust
    /// Gather the whole record as a single ordered [`FieldValue::List`] — the
    /// typed image of C++ `getData(void *rec)`'s offset-addressed walk. Only
    /// **data-bearing** children (those whose [`value`](View::value) is `Some`)
    /// contribute, in child order; a child with no value is the `dataSize == 0`
    /// case and is absent. Built on [`gather_data`](Self::gather_data).
    ///
    /// # Turbo Vision heritage
    /// `TGroup::getData` viewed as producing one record value.
    pub fn gather_list(&self) -> FieldValue {
        FieldValue::List(self.gather_data().into_iter().flatten().collect())
    }

    /// Scatter an ordered [`FieldValue::List`] record back to the data-bearing
    /// children, in child order (the inverse of [`gather_list`](Self::gather_list)).
    /// Children with no value are skipped (they consume no record slot — the
    /// `dataSize == 0` walk). A non-`List` argument is ignored.
    ///
    /// # Turbo Vision heritage
    /// `TGroup::setData` viewed as consuming one record value.
    pub fn scatter_list(&mut self, record: &FieldValue, ctx: &mut Context) {
        let FieldValue::List(items) = record else {
            return;
        };
        let mut next = items.iter();
        for child in self.children.iter_mut() {
            // Only children that carry a value take a slot (faithful to the
            // offset walk: a dataSize==0 control is skipped).
            if child.view.value().is_some() {
                if let Some(v) = next.next() {
                    child.view.set_value_ctx(v.clone(), ctx);
                }
            }
        }
    }
```

- [ ] **Step 4: Run the new tests + gather/scatter suite + clippy**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo test -p tvision-rs --lib -j2 -- --test-threads=2 group::tests
cargo clippy -p tvision-rs --all-targets -j2 -- -D warnings
```
Expected: new `gather_list`/`scatter_list` tests PASS; the existing `gather_data_*`/`scatter_data_*` tests unchanged; clippy clean.

- [ ] **Step 5: Commit**

```bash
git add src/view/group.rs
git commit -m "feat(group): gather_list/scatter_list — the record as an ordered FieldValue::List

The whole-dialog record as one FieldValue::List (the typed image of C++
getData(void*)'s positional offset walk): data-bearing children contribute in
order, dataSize==0 children are absent. Built on the unchanged positional
gather_data/scatter_data primitives. Behavior-preserving (new methods).

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

### Task 4: Documentation (lands WITH the phase, per spec §9)

**Files:**
- Modify: `docs/book/src/apps/dialogs.md` — note the widened `FieldValue` currency + `gather_list`/`scatter_list` ordered record.
- Create: `docs/book/src/apps/extensibility.md` — the §3.5 open story (new page).
- Modify: `docs/book/src/SUMMARY.md` — add the new page under the same section as `dialogs.md`.
- Verify rustdoc: the rustdoc added in Tasks 1–3 satisfies per-item docs; this task adds the guide prose and runs the gates.

**Interfaces:** none (documentation only).

- [ ] **Step 1: Add the extensibility guide page**

Create `docs/book/src/apps/extensibility.md`:

```markdown
# Third-party components & data interchange

tvision-rs gives your own widgets the same unified, *typed* data interchange the
framework's controls use — what C++ Turbo Vision did with `void*`/`getData`, but
without erasing type safety across the board. There are **three open paths**, each
typed at the layer that owns the type.

## 1. A modal that returns a value — `exec_view_with`

A component launched modally returns *any* native type by value (see
[Modal `execView`](../port/modal.html#getting-a-result-back-exec_view_with)). The
result type is yours; the framework never names it.

## 2. Field data — `FieldValue`, including `Custom`

A control exposes its value as a
[`FieldValue`](../api/tvision-rs/data/enum.FieldValue.html). The well-known shapes
(`Text`/`Int`/`Bool`/`Bits`/`List`) interoperate with framework widgets and generic
consumers. For a payload your component invents, use `FieldValue::Custom`:

```rust,ignore
#[derive(Debug, PartialEq)]
pub struct DateRange { pub start: Date, pub end: Date } // export this type!

impl View for DateRangePicker {
    fn value(&self) -> Option<FieldValue> { Some(FieldValue::custom(self.range.clone())) }
}

// the consumer (your code, or anyone who depends on your crate):
let range = fv.value_as::<DateRange>()?;   // loud: a mismatch is a descriptive error
```

`Custom` is **runtime-checked and fail-loud**: `value_as::<T>()` returns a
descriptive `Result` (a mismatch announces itself at first execution), while
`as_custom::<T>()` returns `Option` for `match`-style reads. It is type-*safe* (a
wrong type never misreads — it fails closed) though not compile-*checked* across
the `value()` boundary, because the value crosses the object-safe `dyn View`
boundary. The exported payload type *is* the contract; one test exercising the
producer→consumer exchange pins it. (Caveat: `TypeId` is per-version, so a diamond
dependency pulling your crate at two incompatible versions yields distinct types
that won't cross — handle by dependency discipline.)

A distributable component can offer **both** a typed `Custom(MyType)` and a generic
scalar/`List` projection, so consumers who don't depend on your types can still
read it. And two of your *own* tightly-coupled components can skip `FieldValue`
entirely and share a typed `Rc<RefCell<MyState>>` for full compile-time checking —
`Custom` is only the price of the framework's *generic* plumbing.

## 3. Notification — a custom `Command` broadcast

`Command` is an open newtype: mint your own and broadcast it
(`Event::Broadcast`) to notify siblings; read the data they then expose via path 2.
```

- [ ] **Step 2: Wire it into `SUMMARY.md`**

Read `docs/book/src/SUMMARY.md`, find the line linking `apps/dialogs.md`, and add directly after it (matching the existing indentation):

```markdown
  - [Third-party components](apps/extensibility.md)
```

- [ ] **Step 3: Extend `apps/dialogs.md`**

Read `docs/book/src/apps/dialogs.md` and append a short subsection documenting: `FieldValue` is now the single currency with the well-known shapes plus `Custom`; a group's whole record is one ordered `FieldValue::List` via `gather_list`/`scatter_list` (positional, the C++ `getData` walk); link to `apps/extensibility.md` for the `Custom` seam. Keep any fenced `rust` block as `rust,ignore` unless it can compile under `cargo xtask test` with a hidden `# use tvision_rs as tv;` preamble.

- [ ] **Step 4: Run the doc gates**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo xtask test     # guide rust blocks compile
cargo xtask docs     # regenerate + build + link-check
```
Expected: `cargo xtask test` passes. **Note (pre-existing, from the Phase 1 ledger):** `cargo xtask docs` link-check currently reports ~720 broken `../api/` links on `main` too (the local checker can't resolve rustdoc `api/` links) — this is NOT introduced here. Confirm no *new* broken links beyond that baseline; do not try to fix the pre-existing infra issue in this task (surface it instead).

- [ ] **Step 5: Commit**

```bash
git add docs/book/src/apps/extensibility.md docs/book/src/apps/dialogs.md docs/book/src/SUMMARY.md
git commit -m "docs(guide): FieldValue currency + the third-party extensibility story

New apps/extensibility.md (the three open paths; FieldValue::Custom with the loud
value_as accessor; typed-vs-generic exposure; the TypeId/version caveat;
share-typed-state-directly alternative). Extend apps/dialogs.md with the widened
currency + gather_list/scatter_list ordered record. Per the spec docs-per-phase rule.

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
Expected: green; no snapshot changed (Phase 2 is behavior-preserving).

## Deferred to a follow-on (recorded, not dropped)

- **`inventory`-collected `Program::self_check()` + the `data_self_check` per-component convention** (spec §3.5). It requires adding the external `inventory` crate — a dependency decision for the owner, deliberately out of this plan. The `Custom`/`value_as` core landed here is the substrate it would build on. The dependency-free fallback is unchanged and already usable: a component author writes a `#[cfg(test)]` round-trip exercising its own `value()`/`set_value()`/`Custom`, and an app pins a producer→consumer exchange with one integration test.

## Self-Review notes (author)

- **Spec coverage (Phase 2):** widen `FieldValue` with `Bool`/`Bits`/`List` (Task 1) ✓; `Custom` open seam + `custom`/`as_custom`/loud `value_as` + `FieldTypeError` (Task 1, spec §3.5) ✓; cluster `value()`/`set_value()` → `Bits`, not `ColorPicker` (Task 2, C-1) ✓; ordered-`List` record gather/scatter (Task 3) ✓; docs incl. new `apps/extensibility.md` (Task 4, §9) ✓. `self_check` recorded as a deliberate follow-on (dependency decision).
- **Type consistency:** `FieldValue::Bits(u32)` is produced by clusters (Task 2) and is a variant of the Task-1 enum; `FieldValue::List(Vec<FieldValue>)` is produced/consumed by `gather_list`/`scatter_list` (Task 3) and built on the unchanged `gather_data` (`Vec<Option<FieldValue>>`); `value_as::<T>() -> Result<Rc<T>, FieldTypeError>` and `as_custom::<T>() -> Option<Rc<T>>` use the same `CustomValue::as_any_rc` bridge. Manual `PartialEq` (Custom = `Rc::ptr_eq`) replaces the derive that `Rc<dyn CustomValue>` would have broken; `Debug` stays derived because `dyn CustomValue: Debug`.
- **Behavior-preserving:** new variants/methods/cluster-impls are additive; `gather_data`'s only consumer-tests use `InputLine` (unaffected); no snapshot touched.
