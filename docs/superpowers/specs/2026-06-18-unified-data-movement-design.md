# Unified typed data-movement substrate (design)

**Date:** 2026-06-18
**Status:** design — awaiting owner review → writing-plans
**Origin:** Porting the `tcv` example faithfully (its `Desktop^.ExecView(InfoBox)`)
surfaced consumer-API gap #2 (no generic view-launched modal). Investigating
*why* it wasn't ported revealed a deeper issue: C++'s one loose data-movement
mechanism (`void*`/`infoPtr`, `getData`/`setData`, `message`, `TStreamable`) was
ported into **several different typed solutions at different sites** where one
unified typed mechanism would serve. The owner's objective: **clean it up** — a
single, *truly Rusty*, **typed** (no `Box<dyn Any>`) data-movement substrate that
stays faithful to Turbo Vision and consistent with the port's lingo, with the
generic `ExecView` capability built on it as its first consumer.

---

## 1. Problem: one C++ idiom, many Rust answers

C++ moves data across view/loop boundaries loosely: `TEvent.infoPtr` (`void*`),
`getData`/`setData`/`dataSize` over raw record memory, `message(target, …,
infoPtr)`, `TStreamable`. Rust's type system forbids that looseness, so the port
solved the **same underlying problem** — *"hand a typed payload to a target,
resolved later, across the borrow boundary"* — in several places, several ways.

A read-only inventory found six clusters (file:line citations in the audit
transcript; summarized here):

| Cluster | Problem | Current mechanism(s) | Typing today |
|---|---|---|---|
| **A** | read/write a dialog field | `FieldValue` + `value`/`set_value`; `Group::gather_data`/`scatter_data` | typed enum (`Text`/`Int` only) |
| **B** | keep two siblings in sync (push value to a view by id) | ~15 `Deferred::Sync*`/`Set*` variants + ~40 `as_any` downcasts in `program.rs` | **downcast** (laissez-faire residue) + a few trait hooks |
| **C** | decide which sibling a broadcast is about | `Event::Broadcast { source: ViewId }` + `source == self.x` filter | `ViewId` filter (addressing, not payload) |
| **D** | deliver a modal's result to its launcher **view** | `ModalCompletion::{HistoryPick, RouteModalAnswer, SaveAsPick, FindPick, ReplacePick, ThemeColorPick}` + `set_modal_answer` | per-consumer enum + `as_any` reads |
| **E** | deliver a modal's result to a **`Program` method** | `ColorPick`/`ThemeEdit` via `Rc<Cell>`/`Rc<RefCell>` sinks | shared cell + `as_any` |
| **F** | pure loop-control effects | `Deferred::{PushCapture, EnableCommand, Close, EndModal, …}` | already unified, correct |

**The laissez-faire residue is sharp and locatable:** the ~40
`as_any`/`downcast_mut` broker sites concentrated in `program.rs` (cluster B, and
the read side of D/E). That downcasting *is* the C++ `void*`/`infoPtr` looseness
leaking through an otherwise-typed port. Cleaning it up is the heart of this work.

---

## 2. Goals & non-goals

**Goals**
- One **typed** payload vocabulary and one **addressed delivery** mechanism for
  the overlapping clusters (B, the addressed parts of D, and E).
- **Zero `Box<dyn Any>`.** All type knowledge stays on the consumer side, by
  value (generics) or behind typed accessors.
- Faithful to Turbo Vision (a TV veteran recognizes `message`/`getData`) and
  consistent with the port's lingo (`broker`, `apply_*`, `Deferred`, `request_*`,
  `FieldValue`).
- The generic **`ExecView`** capability (consumer-API gap #2) built on this
  substrate, validated by the `tcv` Info box.
- Remove the ~40 `as_any` broker downcasts, replacing them with a virtual method.

**Non-goals (explicitly NOT unified — see §6)**
- `Event::Broadcast.source` stays a pure subject *filter*, never a payload.
- `gather_data`/`scatter_data` stays an ordered synchronous group-walk.
- Pure loop-control `Deferred` variants (cluster F) stay as effect variants.
- The two modal-result *paths* (view-launched vs `Program`-launched) stay
  distinct — they have genuinely different ownership (see §4.3).
- No serialization/`serde` (in-process hand-off; `TStreamable`→serde stays parked
  for the persistence case only).

---

## 3. The design — three pieces sharing one vocabulary

### 3.1 Widen `FieldValue` (the payload vocabulary)

`src/data.rs` — keep the name and meaning ("the typed unit of data transfer");
add the kinds the current port reaches past `value()` to read via downcast:

```rust
pub enum FieldValue {
    Text(String),
    Int(i32),
    Bool(bool),
    Bits(u32),                       // cluster controls (CheckBoxes/RadioButtons)
    List(Vec<FieldValue>),
    Map(Vec<(String, FieldValue)>),  // a named record (a dialog's fields)
}
```

Then `CheckBoxes`/`RadioButtons` (→ `Bits`) and `ColorPicker` (→ `Bits` of packed
RGB) implement `value()`/`set_value()` **honestly**, eliminating the `as_any`
reads in `FindPick`/`ReplacePick`/`ThemeColorPick` (a single `Color` packs into
`Bits`). A whole **`Theme`** is a large struct that does **not** pack cleanly into
`FieldValue` and is deliberately handled by the by-value path in §3.3 (not forced
into `FieldValue`).

### 3.2 `BrokerMsg` + `Deferred::Broker` + `View::apply_broker` (the spine)

Replace cluster B's ~15 `Deferred` variants and ~40 `as_any` broker downcasts
with **one typed payload addressed to a `ViewId`, delivered through a virtual
method** — the port's "broker" concept made first-class:

```rust
// src/view/context.rs — the typed broker payload (one variant per sync kind)
pub enum BrokerMsg {
    ScrollerDelta { dx: i32, dy: i32 },
    ScrollBarParams { value: i32, min: i32, max: i32, page: i32, arrow: i32 },
    IndicatorPos { location: Point, modified: bool },
    ListScroll { h: Option<i32>, v: Option<i32> },
    OutlineDelta { /* … */ },
    EditorDelta { /* … */ },
    MenuCurrent(Option<usize>),
    ButtonDefault(bool),
    FocusedFile(/* a typed record, not a downcast */),
    PageStackActivate(/* … */),
    SplitterDivider(DividerOp),
    ModalResult { command: Command, value: Option<FieldValue> }, // cluster D, see §3.3
    Field(FieldValue),                                           // app/consumer payloads
}

// one unified deferred variant (replaces the Sync*/Set* family)
Deferred::Broker { target: ViewId, msg: BrokerMsg }

// src/view/view.rs — the receiver hook (parallels apply_list_scroll; NO downcast)
fn apply_broker(&mut self, _msg: &BrokerMsg, _ctx: &mut Context) {}
```

The pump's deferred-drain resolves `group.find_mut(target)` and calls
`apply_broker(&msg, ctx)` — a **virtual dispatch to the concrete widget**, never a
framework downcast. Each sync widget implements the arm(s) it understands. The
existing trait-hook brokers (`apply_list_scroll`, `set_menu_current`,
`update_menu_commands`) fold in as `BrokerMsg` arms; the existing
`Context::request_*` helpers (`request_sync_scroller_delta`, …) keep their
signatures but queue `Deferred::Broker{…}` underneath.

*Naming note (owner to confirm):* `BrokerMsg` / `Deferred::Broker` /
`apply_broker` follow the established "broker" term and the `apply_*` hook style.
Alternative for the hook verb considered: `deliver`. The payload-enum style
matches `Deferred`'s PascalCase variants.

### 3.3 Modal results on the spine + generic `exec_view_with`

**View-launched modals (cluster D)** ride the spine: `apply_modal_completion`
extracts the finished modal's result as a `BrokerMsg::ModalResult { command,
value }` (reading `value()` / structured `FieldValue` where the child implements
it; one explicit, single-site downcast only where the result is a whole concrete
type the framework owns) and calls `requester.apply_broker(&msg, ctx)`. This
collapses `RouteModalAnswer`/`FindPick`/`SaveAsPick`/`ReplacePick`/`ThemeColorPick`
into "extract a `ModalResult`, broker it to the requester." `set_modal_answer`
becomes the `command`-only case of `ModalResult`.

**`Program`-launched modals (cluster E)** use a generic by-value return:

```rust
// src/app/program.rs
pub fn exec_view_with<R>(
    &mut self,
    view: Box<dyn View>,
    extract: impl FnOnce(&mut dyn View, Command) -> R,
) -> R
```

The closure runs in the post-loop/pre-drop window, reads the finished modal, and
returns the caller's **own `R` by value** — `R` is caller-named, the framework
never sees it. This **deletes** both `Rc<Cell<Color>>`/`Rc<RefCell<Theme>>` sinks
and the `ColorPick`/`ThemeEdit` `ModalCompletion` variants. `color_dialog` /
`theme_editor` switch to it.

### 3.4 Generic `ExecView` (gap #2) — the first consumer

- **View-launched:** `Context::request_exec_view(view: Box<dyn View>, requester:
  ViewId, then_command: Option<Command>)` queues `Deferred::OpenModal(view, …)`
  (the PORTING-GUIDE D9 pre-named plan). The pump stashes it in `pending_modal`
  (which **already holds `Box<dyn View>`**), runs it via the existing
  single-loop modal machinery, and on close brokers a
  `BrokerMsg::ModalResult` back to `requester` and re-injects `then_command`.
- **`Program`-launched:** `exec_view_with<R>` (§3.3) already covers it.

For the `tcv` Info box (read-only, OK button = `cmCancel`), only the `command`
arm is exercised — but the typed `value` channel is there for input dialogs.

---

## 4. Why this is the right shape

### 4.1 Truly Rusty + zero `dyn Any`
Type knowledge stays on the consumer side three ways: by-value generics
(`exec_view_with<R>`), typed enum payloads (`BrokerMsg`/`FieldValue`), and virtual
dispatch (`apply_broker`). The one theoretical case none serve by value — *a
downward-borrowed view launching a modal that returns a third-party's native
struct* — **does not exist in the codebase**; if it ever arises it is bounded by a
typed-accessor trait, never `dyn Any`. (Documented boundary, per the
"ported-or-deliberately-not-with-reason" rule.)

### 4.2 More faithful to TV, not less
`apply_broker` *is* `message(view, …, infoPtr)` with `infoPtr` reinstated as a
**typed** payload and the receiver as a real method — i.e. `handleEvent`-style
dispatch. Widening `FieldValue` is the typed successor to `getData`/`setData`
(D10). A TV veteran recognizes the model; only the wire type is tightened.

### 4.3 What the ownership split forces
A `Program` method *can* hold the `exec_view_with` closure; a downward-borrowed
view *cannot* (it owns no `&mut Program`). So the two modal-result paths stay
distinct **by necessity**, each now minimal: closure-by-value for top-level,
`BrokerMsg::ModalResult` for view-launched. This is not duplication — it is two
drain sites feeding one vocabulary.

---

## 5. Migration plan (full cleanup, staged for safe execution)

Scope is the **full** cleanup; execution is staged so each step is
behavior-preserving and snapshot-verified (never a big-bang):

- **Phase 1 — vocabulary:** widen `FieldValue`; add honest `value()`/`set_value`
  on `CheckBoxes`/`RadioButtons`/`ColorPicker`. Snapshot-verify dialogs unchanged.
- **Phase 2 — the spine:** add `BrokerMsg`, `Deferred::Broker`,
  `View::apply_broker` (+ macro forwarder). Migrate cluster B widget-by-widget:
  each `Deferred::Sync*`/`Set*` + its `program.rs` downcast → a `BrokerMsg` arm +
  `apply_broker` impl. After each widget, run its snapshot + the integration
  tests. Remove the dead `Deferred` variant once its last user is migrated.
- **Phase 3 — `exec_view_with<R>`:** introduce it; migrate `color_dialog` /
  `theme_editor`; delete the `Rc` sinks + `ColorPick`/`ThemeEdit` variants.
- **Phase 4 — modal results on the spine:** convert cluster D completions to
  `BrokerMsg::ModalResult`; reduce `ModalCompletion` to the routing it still needs
  (the `OpenModal` request + the requester id + `then_command`).
- **Phase 5 — generic `ExecView`:** `request_exec_view` + `Deferred::OpenModal`;
  make `tcv`'s Info box a real custom Dialog launched from the list. Snapshot.

Each phase is independently shippable and reviewed (subagent-driven + two-stage
review). Phases 1–4 are behavior-preserving refactors; Phase 5 adds the new
capability. The ~40 downcasts retire across Phase 2 + Phase 4.

---

## 6. What stays separate (resisting over-unification)

1. **`Event::Broadcast.source`** stays a subject *filter* — folding payload in
   re-creates the `infoPtr` polymorphism the port deliberately killed (D4).
2. **`gather_data`/`scatter_data`** stays an ordered synchronous group-walk — its
   semantics are *positional record marshalling* (faithful to `getData` summing
   `dataSize` in child order); addressing it by id would lose the order contract.
3. **Cluster-F loop-control `Deferred` variants** stay effect variants — they
   touch capture-stack / command-set / tree-structure / loop-state, not "value to
   a view." Only the *payload-carrying* subset (cluster B) folds into `Broker`.
4. **The two modal-result paths** stay distinct (§4.3).

---

## 7. Risks

- **Breadth (Phase 2/4):** ~40 downcast sites + every sync widget. Mitigation:
  strictly widget-by-widget, behavior-preserving, snapshot-verified after each;
  the `delegate` macro spy test guards the new trait method's forwarders.
- **`FieldValue` for `Theme`:** a `Color` packs into `Bits`, but a whole `Theme`
  is a large struct that does not pack cleanly; it is intentionally returned by
  value via `exec_view_with<R>`, NOT shoehorned into `FieldValue`. Keep that
  boundary.
- **`BrokerMsg` becomes a wide enum:** but it *replaces* an equally wide set of
  `Deferred` variants + downcasts — net more uniform, fewer concepts.

---

## 8. Faithfulness map (C++ → this design)

| C++ | This design |
|---|---|
| `message(target, evBroadcast/evCommand, cmX, infoPtr)` | `Deferred::Broker{target, BrokerMsg}` → `apply_broker` (typed `infoPtr`, method receiver) |
| `getData`/`setData`/`dataSize` | widened `FieldValue` + `value`/`set_value`; `gather_data`/`scatter_data` (unchanged) |
| `execView(p)` returns command, view readable | view-launched: `request_exec_view` → `BrokerMsg::ModalResult`; top-level: `exec_view_with<R>` by value |
| `infoPtr` as subject | `Event::Broadcast.source` (unchanged) |
| `TStreamable` | out of scope (persistence only; stays parked) |
