# Unified typed data-movement substrate (design)

**Date:** 2026-06-18
**Status:** design v2 (incorporates adversarial review) — awaiting owner review → writing-plans
**Origin:** Porting the `tcv` example faithfully (its `Desktop^.ExecView(InfoBox)`)
surfaced consumer-API gap #2 (no generic view-launched modal). Investigating
*why* it wasn't ported revealed a deeper issue: C++'s one loose data-movement
mechanism (`void*`/`infoPtr`, `getData`/`setData`, `message`, `TStreamable`) was
ported into **several different typed solutions at different sites**, including
~40 `as_any` downcasts in `program.rs` — the one un-Rusty residue in an otherwise
typed port. Objective (owner): **clean it up** — a single typed *currency* and one
*right mechanism per kind of data movement*, applied **uniformly** (the framework
is pre-release; retrofit cost is immaterial), faithful to Turbo Vision and
consistent with the port's lingo. The generic `ExecView` capability is built on it.

> **v2 note.** v1 proposed a `BrokerMsg` god-enum + `Deferred::Broker` +
> `apply_broker`. An adversarial TV-porting/Rust-framework review rejected it:
> (a) it was a god-enum forcing every widget into `match { _ => {} }`, conserving
> (not reducing) concept count; (b) it created a *second* structured-payload
> vocabulary overlapping the widened `FieldValue`; (c) the `exec_view_with`
> closure as written was a use-after-free-shaped bug; (d) "zero `dyn Any` / ~40
> downcasts gone" was dishonest (multi-view dialogs don't fit a single-`self`
> hook). v2 drops `BrokerMsg`, centers on `FieldValue` as the data currency,
> reuses the existing defaulted-`View`-method pattern for sync signals, fixes the
> closure, and states honest scope.

---

## 1. Problem: one C++ idiom, many Rust answers

C++ moves data across view/loop boundaries loosely: `TEvent.infoPtr` (`void*`),
`getData`/`setData`/`dataSize` over raw record memory, `message(target, …,
infoPtr)`, `TStreamable`. Rust forbids that looseness, so the port solved the
problem in several places, several ways. Inventory (file:line in the audit
transcript), six clusters:

| Cluster | Problem | Current mechanism | Typing today |
|---|---|---|---|
| **A** | read/write a dialog field | `FieldValue` + `value`/`set_value`; `Group::gather_data`/`scatter_data` | typed enum (`Text`/`Int` only) |
| **B** | keep two siblings in sync (push a signal to a view by id) | ~10 `Deferred::Sync*`/`Set*` variants; ~half via `as_any` downcast, ~half already via a defaulted `View` method (`apply_list_scroll`, `set_menu_current`, `update_menu_commands`) | mixed: downcast + trait method |
| **C** | decide which sibling a broadcast is about | `Event::Broadcast { source: ViewId }` + `source == self.x` filter | `ViewId` filter (addressing, not payload) |
| **D** | deliver a modal's result to its launcher **view** | `ModalCompletion::{HistoryPick, RouteModalAnswer, SaveAsPick, FindPick, ReplacePick, ThemeColorPick}` + `set_modal_answer` | per-consumer enum; multi-child `as_any` reads |
| **E** | deliver a modal's result to a **`Program` method** | `ColorPick`/`ThemeEdit` via `Rc<Cell>`/`Rc<RefCell>` sinks | shared cell + `as_any` |
| **F** | pure loop-control effects | `Deferred::{PushCapture, EnableCommand, Close, EndModal, …}` | already unified, correct |

**The laissez-faire residue:** the `as_any`/`downcast_mut` reads in clusters B, D,
E. They are the typed port's nearest thing to C++ `void*`. Cleaning them is the goal.

**Key insight from the review:** there is **not one** unifying mechanism — there
are **two distinct kinds** of data movement that must stay distinct (folding sync
signals into the field-data currency is the over-unification v1 committed):

- **Field/record data** (cluster A; the *read side* of D) — small, named,
  marshalled values that cross into a view that can't receive a native type.
- **Sync signals** (cluster B) — "this sibling changed; recompute" — not field
  data; a behavioral poke at a known view.

DRY means *one way to do each kind*, not one mechanism for both.

---

## 2. Goals & non-goals

**Goals**
- **`FieldValue` is the single typed data currency** (widened) — the `getData`/
  `setData` successor. Every data-bearing view implements `value()`/`set_value()`.
- **Sync signals deliver through defaulted per-capability `View` methods**
  (extend the `apply_list_scroll` pattern), so **no sync site downcasts in the
  pump**.
- **`exec_view_with<R>`** returns top-level modal results **by native value**
  (deletes the `Rc` sinks).
- **Generic `ExecView`** (`request_exec_view` + `Deferred::OpenModal`) for gap #2,
  validated by `tcv`'s real Info box.
- Applied **broadly but with judgment** (pre-release, so retrofit cost is no
  excuse to skip a genuine improvement) — see §2.1.
- **No `Box<dyn Any>` in the data path.** (Honest scope below.)

### 2.1 Guiding principle: apply where it helps, not where it burdens

Churn is cheap pre-release, so "it's a lot of sites" is **not** a reason to skip a
real cleanup. But "apply it everywhere" is **not** a licence to force the concept
into places it fits badly. Convert a site **only when** doing so (a) removes a real
downcast or duplication **and** (b) the result reads as naturally as (or better
than) what it replaces. **Do not** force-fit where it becomes a burden:

- don't route a single-field control's value through `FieldValue::Map` — a plain
  `Text`/`Int` is the natural shape;
- don't mint a defaulted `View` method for a genuinely one-off, single-caller sync
  poke if a method makes the trait noisier than the downcast it removes — weigh
  trait-surface cost against the downcast removed;
- don't contort a genuinely multi-view, concrete-type orchestration into a
  `FieldValue` shape just to claim a downcast was removed;
- don't pull non-data structural pushes (parent→child display state) into the data
  path (§6.6).

The test is *"is this site clearer after?"*, not *"is this site converted?"* Any
site deliberately **left as-is records the one-line reason** (the house
"ported-or-deliberately-not-with-reason" rule), so a reader sees the boundary was a
decision, not an omission.

**Non-goals (explicitly NOT unified)**
- `Event::Broadcast.source` stays a subject *filter* (cluster C), never a payload.
- `gather_data`/`scatter_data` stays an ordered synchronous group-walk producing/
  consuming a `FieldValue` record (faithful to `getData` summing `dataSize`).
- Cluster-F loop-control `Deferred` variants stay effect variants.
- The two modal-result paths (view-launched vs `Program`-launched) stay distinct —
  forced by ownership (§4.2).
- **Non-data structural `as_any` is out of scope** and stays: parent→child display
  pushes (`Frame::set_flags`/`set_zoomed`, `Window`→frame), `desktop_insert`,
  FileDialog readback. These are "a parent reaches a *known* child to push display
  state," a different category from data movement. We do **not** claim to remove them.
- No serialization/`serde` (in-process; `TStreamable`→serde stays parked for persistence).

---

## 3. The design

### 3.1 Widen `FieldValue` (the one data currency)

`src/data.rs` — keep the name ("the typed unit of data transfer"); widen so it can
carry every field kind and a whole record:

```rust
pub enum FieldValue {
    Text(String),
    Int(i32),
    Bool(bool),
    Bits(u32),                       // cluster controls; a packed Color
    List(Vec<FieldValue>),
    Map(Vec<(String, FieldValue)>),  // a named record == C++ getData(void *rec)
}
```

Every data-bearing view implements `value()`/`set_value()` honestly:
`CheckBoxes`/`RadioButtons` → `Bits`, `ColorPicker` → `Bits` (packed RGB), and a
`Dialog`/`Group` gathers its children into a `Map` via `gather_data` (and scatters
a `Map` back via `scatter_data`). `Map` is the typed image of C++ `getData(void
*rec)` — so a whole dialog's result is one `FieldValue::Map`, **read without
downcasting any child**. A whole `Theme` is the one value too large/structured to
pack and is returned by the by-value path (§3.3), not via `FieldValue`.

### 3.2 Sync signals → defaulted per-capability `View` methods (no god-enum)

Cluster B is **not** field data; it keeps its per-kind `Deferred` variants (the
house "add-a-variant" rule), but each **delivers through a defaulted `View`
method** instead of a pump downcast — extending the pattern the port already uses
for `apply_list_scroll`/`set_menu_current`/`update_menu_commands`:

```rust
// src/view/view.rs — one defaulted method per sync capability (no Self params; object-safe)
fn apply_scroll_delta(&mut self, _dx: i32, _dy: i32, _ctx: &mut Context) {}
fn set_scroll_params(&mut self, _p: ScrollParams) {}
fn apply_outline_delta(&mut self, _d: i32, _ctx: &mut Context) {}
fn apply_editor_delta(&mut self, _d: EditorDelta, _ctx: &mut Context) {}
fn set_indicator_pos(&mut self, _loc: Point, _modified: bool) {}
// … one per existing Deferred::Sync*/Set* kind
```

The pump's deferred-drain resolves `group.find_mut(target)` and calls the typed
method — **virtual dispatch to the concrete widget, never a downcast**. Each method
is defaulted, so widgets implement only the one(s) they answer (the compiler names
it; no `match { _ => {} }` arm-dropping). Cluster-B downcast sites retire this way
**where the resulting method reads as naturally as the downcast it removes**
(§2.1) — the common case; a genuinely one-off poke may stay if a method would only
add trait noise, with its reason recorded. **Each new `View` method needs a forwarder in
`tvision-rs-macros/src/specs.rs` AND an entry in `tests/delegate_view.rs`** (the
spy test) — mechanical, mirroring the existing `apply_list_scroll` forwarder.

*(Rejected: a single `apply_broker(&BrokerMsg)` god-enum — it conserves variant
count, forces arm-dropping, and duplicates the `FieldValue` payload vocabulary.)*

### 3.3 Modal results: `FieldValue` for views, `exec_view_with<R>` for `Program` methods

**View-launched modals (cluster D)** read the finished modal's result as a
`FieldValue` (a field via `value()`, or the whole record via `gather_data` →
`Map`) and deliver it to the requester by id — **no multi-child downcast**.
`set_modal_answer(Command)` stays for the command-only/decision case;
`set_value(FieldValue)` (or a `set_modal_data(FieldValue)` sibling) delivers the
typed result. Find/Replace stop downcasting `CheckBoxes`/`InputLine` children and
instead `gather_data` the modal into a `Map` the editor consumes. The multi-view
*routing* (which editor to write) is by-id and stays; only the *reads* go
downcast-free. `ThemeColorPick` folds into the theme-editor view recomposing its
own style from the delivered `Color` (the "second view" it reads is itself).

**`Program`-launched modals (cluster E)** use a generic by-value return:

```rust
// src/app/program.rs — the extract closure is threaded INTO exec_view_with_completion
// and invoked at the pre-drop window (program.rs:~1442), NOT after it returns
// (the modal is removed+dropped at ~1463). It receives the modal's OWN &mut dyn View
// only; the existing end_state save/restore (re-entrancy) is inherited.
pub fn exec_view_with<R>(
    &mut self,
    view: Box<dyn View>,
    extract: impl FnOnce(&mut dyn View, Command) -> R,
) -> R
```

`R` is caller-named and returned by value — the framework never names it (no
`dyn Any`, no `Rc` cell). `color_dialog`/`theme_editor` switch to it; the
`ColorPick`/`ThemeEdit` `ModalCompletion` variants and both `Rc<Cell>`/
`Rc<RefCell>` sinks are **deleted**. (A *view* wanting a big native result — none
exists today — would use a consumer-owned `Rc<RefCell<T>>` it passes into the
modal; documented escape hatch, no framework change.)

### 3.4 Generic `ExecView` (gap #2) — the first consumer

- **View-launched:** `Context::request_exec_view(view: Box<dyn View>, requester:
  ViewId, then_command: Option<Command>)` queues `Deferred::OpenModal(view, …)`
  (the PORTING-GUIDE D9 pre-named plan). The pump stashes it in `pending_modal`
  (already `Box<dyn View>`), runs it via the existing single-loop machinery, and
  on close delivers the result to `requester` (command via `set_modal_answer`,
  data via `FieldValue`) and re-injects `then_command`.
- **`Program`-launched:** `exec_view_with<R>` (§3.3).

`tcv`'s Info box (read-only, OK = `cmCancel`) exercises only the command path; the
`FieldValue` path serves input dialogs.

---

## 4. Why this is right

### 4.1 DRY, honestly
One currency for field data (`FieldValue`), one mechanism for sync (defaulted
`View` methods), one by-value path for top-level results — *one way per kind*, not
one mechanism for all (which was the v1 god-enum mistake) and not two vocabularies
for the same kind (the v1 `FieldValue`-vs-`BrokerMsg` overlap). `FieldValue::Map`
genuinely removes the cluster-D multi-child read downcasts; the trait-method sync
genuinely removes the cluster-B downcasts.

### 4.2 The ownership split is forced
A `Program` method *can* hold the `exec_view_with` closure; a downward-borrowed
view *cannot* (no `&mut Program`). So the two modal-result paths stay distinct by
necessity — closure-by-value for top-level, `FieldValue`/command for view-launched.

### 4.3 Faithful to TV
`FieldValue::Map` is the typed image of `getData(void *rec)`; `value`/`set_value`
are `getData`/`setData`; `gather_data`/`scatter_data` are `TGroup::getData/setData`
(child-order walk). Sync via a defaulted method is the **deferred, return-less
successor to `message(view, …, infoPtr)`** (the D3/D9 deviation — C++ `message`
is synchronous and returns `void*`; ours defers and returns nothing). `exec_view_with`
is C++ `execView` returning a value to a method caller. `Event::Broadcast.source`
is `infoPtr`-as-subject, unchanged.

### 4.4 Honest scope (no overclaim)
Removes the **data-movement** downcasts (clusters B and D-reads). Does **not**
remove non-data structural `as_any` (parent→child display pushes, `desktop_insert`,
FileDialog readback) — a different category, kept and documented. "Zero `dyn Any`"
applies to the data path only.

---

## 5. Migration plan (uniform cleanup, staged; behavior-preserving per phase)

Reordered per the review — **the cleanest, highest-value phase lands first as the
proof of value.** Each phase is subagent-driven + two-stage-reviewed, snapshot-verified.

- **Phase 1 — `exec_view_with<R>` (proof of value, cleanest).** Add it (closure
  threaded into `exec_view_with_completion`, invoked pre-drop). Migrate
  `color_dialog`/`theme_editor`; delete the two `Rc` sinks + `ColorPick`/`ThemeEdit`
  variants. Single-view result, sound borrow. Highest reduction-per-risk.
- **Phase 2 — widen `FieldValue` + honest `value()`.** Add `Bool`/`Bits`/`List`/
  `Map`; implement `value()`/`set_value()` on `CheckBoxes`/`RadioButtons`/
  `ColorPicker`; make `gather_data`/`scatter_data` produce/consume `Map`. Snapshot
  dialogs unchanged.
- **Phase 3 — sync signals → trait methods.** Widget-by-widget: add the defaulted
  `View` method (+ `specs.rs` forwarder + `delegate_view` entry), move the pump's
  downcast call to the method, verify, repeat. Retire each cluster-B downcast as
  its widget migrates. Genuinely incremental.
- **Phase 4 — modal-result reads via `FieldValue`.** Convert `FindPick`/
  `ReplacePick`/`ThemeColorPick` to read the modal via `value()`/`gather_data`
  (Map) + deliver by id; drop the multi-child downcasts. Honest: the *routing*
  stays; the *reads* go downcast-free. (Not a "free melt into a generic arm" — a
  real per-consumer conversion to the `FieldValue` read path.)
- **Phase 5 — generic `ExecView`.** `request_exec_view` + `Deferred::OpenModal`;
  make `tcv`'s Info box a real custom Dialog launched from the list. Snapshot.

Phases 1–4 are behavior-preserving refactors; Phase 5 adds the capability. Docs
land WITH each phase (§9), never trailing.

---

## 6. What stays separate (resisting over-unification)

1. **`Event::Broadcast.source`** — subject filter, never payload (re-adding payload
   re-creates the `infoPtr` polymorphism the port killed, D4).
2. **Sync signals vs field data** — kept as two mechanisms (trait methods vs
   `FieldValue`); folding sync into `FieldValue` was the v1 over-unification.
3. **`gather_data`/`scatter_data`** — ordered synchronous group-walk (faithful
   `getData`), not id-addressed delivery.
4. **Cluster-F loop-control `Deferred` variants** — effect variants, not data.
5. **The two modal-result paths** — distinct by ownership (§4.2).
6. **Non-data structural `as_any`** — parent→child display pushes, `desktop_insert`,
   FileDialog readback: a different category, kept (§4.4).

---

## 7. Risks

- **Breadth (Phases 2–4):** every data control + every sync broker + the modal
  completions. Mitigation: strictly widget-by-widget, behavior-preserving,
  snapshot-verified after each; the `delegate_view` spy test guards each new trait
  method's forwarder.
- **`FieldValue` for `Theme`:** a `Color` packs into `Bits`, but a whole `Theme`
  does not pack cleanly — intentionally returned by value via `exec_view_with<R>`,
  not shoehorned into `FieldValue`. Keep that boundary.
- **Trait-surface growth:** ~10 new defaulted `View` methods for sync. Accepted:
  defaulted (widgets implement only what they answer), compiler-guided, and the
  honest idiomatic shape (vs a god-enum). Each needs a macro forwarder + spy entry.
- **`Map` ordering contract:** `gather_data`/`scatter_data` are positional/ordered
  (faithful `getData`); a keyed `Map` must preserve child order where scatter
  relies on it. Keep order-stable.

---

## 8. Faithfulness map (C++ → this design)

| C++ | This design |
|---|---|
| `getData`/`setData(void *rec)` / `dataSize` | widened `FieldValue` (`Map` = the record) + `value`/`set_value`; `gather_data`/`scatter_data` |
| `message(target, evBroadcast/evCommand, cmX, infoPtr)` | per-capability defaulted `View` method delivered via `Deferred::Sync*` (deferred, return-less successor — D3/D9) |
| `infoPtr` as subject | `Event::Broadcast.source` (unchanged) |
| `execView(p)` returns to a method caller | `exec_view_with<R>` (by value) |
| `execView(p)` from within a view | `request_exec_view` → result via `FieldValue`/command |
| `TStreamable` | out of scope (persistence only; stays parked) |

---

## 9. Documentation (part of "done", per phase)

New **public** API (`request_exec_view`, `exec_view_with`, the widened
`FieldValue`, the new sync `View` methods for widget authors) and a new
**conceptual model** (field-data-currency vs sync-signal vs by-value-result; the
three-channel modal-result rule). The mdBook guide (`docs/book/src/`) and rustdoc
update **as part of the phase that lands each piece**. Conventions: rustdoc is
user-facing (strip porting bookkeeping; quarantine C++ lineage into a `# Turbo
Vision heritage` section); new guide ```` ```rust ```` blocks need a hidden
`# use tvision_rs as tv;` and compile under `cargo xtask test`.

**Guide chapters to update (extend existing pages):**
- **`internals/brokering.md`** ("Cross-view brokering & ViewId") — the conceptual
  home: the sync-signal-via-defaulted-`View`-method model (no pump downcasts), and
  why sync is separate from field data. (Phase 3)
- **`apps/dialogs.md`** ("Dialogs & data") — the widened `FieldValue` as the data
  currency, `gather`/`scatter` records (`Map`), and the consumer recipe "build a
  custom modal, exec it, read its result," with the modal-result decision rule
  (view→`FieldValue` / top-level→`exec_view_with<R>` / view-wanting-big-native→`Rc<RefCell>`). (Phases 2, 5)
- **`port/modal.md`** ("Modal execView → one loop") — `request_exec_view` +
  `exec_view_with<R>`, with `tcv`'s Info box as the worked example via
  `{{#rustdoc_include}}`. (Phases 1 & 5)
- **`internals/custom-view.md`** ("Writing your own View") — widget authors
  implement the sync `View` methods + `value()`/`set_value()` instead of relying
  on a framework downcast. (Phases 2, 3)
- **`port/handles.md`** ("Pointers & infoPtr → handles") — refine: subject stays
  `Event::Broadcast.source`; field data is `FieldValue`; sync is a typed method. (Phase 3)
- **`port/deferred.md`** + **`internals/deferred.md`** — the `Deferred::Sync*`
  variants now deliver via trait methods; new `Deferred::OpenModal`. (Phases 3, 5)
- **`reference/symbol-map.md`** / **`reference/deviations.md`** — add new symbols
  (`request_exec_view`, `exec_view_with`, the sync methods, the `FieldValue`
  variants) and note the consolidation against D3/D4/D9/D10.

**Rustdoc** on every new public item, with the heritage-section convention.

**Verification (each phase):** `cargo xtask test` (guide doctests compile),
`cargo xtask docs` (regenerate + build + link-check), and
`grep -rl rustdoc_include docs/book/book` empty after any `{{#rustdoc_include}}` edit.
