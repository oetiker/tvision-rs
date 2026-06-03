# Session handover — Row 49 `TMenuView` passive layer DONE (Phase 4). Next (per PORT-ORDER): Row 50 `TMenuBar` / 51 `TMenuBox` / 52 `TMenuPopup` (the D9 popup-exec path + drawing + `execute()`), then status line 47/53

> Living handover for the **next** rstv session. Read this, then
> [CLAUDE.md](file:///home/oetiker/checkouts/rstv/CLAUDE.md) (orientation /
> Current state / Next step), then start. When the next stage lands, update or
> replace this file for the session after.
>
> **Direction = [`docs/PORT-ORDER.md`](file:///home/oetiker/checkouts/rstv/docs/PORT-ORDER.md).**
> It is dependency-ordered; follow it in sequence rather than treating "tracks" as
> an open choice. Lowest-numbered incomplete rows are the work. The
> "Parallelizable batches" section (e.g. Batch C validators 58–62) lists fan-outs
> that *may* run concurrently — an efficiency, not a competing direction.

## Where things stand (git `main`)

| commit | what |
|--------|------|
| `dfe66b1` | **TMenuView passive layer (49)** — command-graying broker + hotkey dispatch (FOUNDATION) ← THIS session |
| `c5c061d` | **TMenu data tree (46)** — `MenuItem`/`Menu`/`MenuBuilder` (FOUNDATION) |
| `fc66637` | **TListBox (48)** — first concrete `TListViewer` (MECHANICAL) |
| `3e6645f` | **TApplication (32)** — thin D2 wrapper over `Program` (MECHANICAL) |
| `47894f0…66ab55f` | **`#[delegate]` proc-macro** — `tvision-macros` crate + workspace, then **adopted** across cluster/Window/Dialog/ParamText/Label/Desktop + the hello example (replaces `cluster_wrapper!`) |

**Build state:** 510 lib + 5 integration (3 `render_pipeline` + 2 `delegate_view`)
+ 2 doctests green; `cargo clippy --workspace --all-targets -- -D warnings` and
`cargo fmt --all --check` clean. **It is a Cargo workspace** (`tvision` +
`tvision-macros`) — use `--workspace` for test/clippy/fmt. (Cargo artifacts land in
`/home/oetiker/scratch/cargo-target` — set `CARGO_TARGET_DIR`.)

**Phase 2 COMPLETE. Batch B (Phase-3 leaves) COMPLETE. Phase-1 row 32 COMPLETE.**
**Phase 4 in progress — Row 46 `TMenu` data tree + Row 49 `TMenuView` passive
layer DONE** (row 49 = THIS session, see below; row 46 + the `#[delegate]` macro +
row 32 landed in prior sessions, sections further down).
**Next incomplete in PORT-ORDER sequence: Row 50 `TMenuBar`** (then 51 `TMenuBox`,
52 `TMenuPopup`), then status line 47/53. Batch C concrete validators 58–62 are an
available parallel fan-out.

> **Worktrees** live under `/scratch/oetiker/claude-worktrees/<project>-<name>`
> (global CLAUDE.md). A `WorktreeCreate` hook (`~/.claude/settings.json` →
> `~/.claude/worktree-create.sh`) redirects the Agent/Workflow
> `isolation:"worktree"` worktrees there, so **isolation IS usable** — BUT the
> hook only activates on a session **restart** (hooks load at startup); until
> then, isolation lands in the project's `.claude/worktrees/` and you should
> create the worktree manually at the `/scratch` path + dispatch a non-isolated
> subagent.

## What landed THIS session — Row 49 `TMenuView` passive layer (FOUNDATION)
The **passive (non-modal) layer** of `TMenuView` (`tmnuview.cpp`): command-graying
+ hotkey-accelerator dispatch, **no drawing / no modal loop** (those are 50–52).
`src/menu/menu_view.rs`. Built main-thread/Opus-orchestrated: advisor-vetted brief
(`docs/briefs/row49-tmenuview.md`) → Opus implementer → **full two-stage review**
(spec then quality, fresh C++-adversarial Opus agents, both PASS) → one MINOR
doc-link fix → integrate. **Scope was deliberately split** (advisor-confirmed): the
interactive `execute()` modal loop maps to the *unbuilt* D9 view-triggered
async-modal path and lands with the drawing subclasses.

- **Command-graying = a BROKER, NOT a `Context` read-accessor** (this **overturned**
  the prior HANDOVER note that said "add a read accessor on `Context`"). The command
  set lives on `Program`; the pump's apply-phase `Context` is alive across a loop
  whose `EnableCommand`/`DisableCommand` arms mutate `command_set` (`&mut`), so a
  `&CommandSet` on `Context` would alias that borrow (+ would add a `Context::new`
  param at every call site). Instead: new **`Deferred::UpdateMenu(ViewId)`** +
  **`Context::request_update_menu`** + defaulted **`View::update_menu_commands(&mut
  self, &CommandSet)`** (no-op default), applied in the pump where `group` and
  `command_set` are **disjoint destructured fields** (no `ctx` needed). The exact
  `Deferred::SyncListViewer` + `View::apply_list_scroll` precedent — *a new deferred
  capability ADDS A VARIANT*. Delegate forwarder added to `tvision-macros/specs.rs`
  + the `delegate_view` spy test (count 21→22).
- **`updateMenu` ported** as `menu_view::update_menu_commands(&mut Menu, &CommandSet)`:
  recurse submenus, `disabled = !cs.has(command)` on command items only (never a
  submenu's own flag), skip separators. The C++ `Boolean` return is **dropped** (D8
  whole-tree redraw makes `if updateMenu drawView` moot; the guarded write collapses
  to an unconditional flip).
- **`hotKey`/`findHotKey` ported** as `menu_view::hot_key(&Menu, KeyEvent) ->
  Option<Command>`: depth-first, skip separators, recurse submenus **regardless of
  the submenu's own `disabled`** (C++ `!disabled` guard is only on the command
  branch), match a command item iff `!disabled && key_code == Some(key)`. The passive
  `evKeyDown` handler posts the matched command. **The C++ `commandEnabled(p->command)`
  re-check is dropped** — safe because (a) the cached `disabled` is kept current by
  the broker and `hot_key` already filters it, and (b) the pump's `drop_disabled`
  boundary filter drops a stale-enabled post; only a one-idle-cycle staleness window
  remains (documented).
- **evBroadcast mask is MOOT** — `Group::handle_event` fans broadcasts to **every**
  child unconditionally (test `broadcast_reaches_all_children_including_disabled`), so
  the C++ `eventMask |= evBroadcast` opt-in needs no port; no gate added.
- **`MenuViewState { state, menu }`** is the embed target for 50/51. **No `MenuView`
  trait yet** and **`current`/`parentMenu` omitted** (omit-until-consumer: only
  `execute()`/`trackMouse`/`getHelpCtx` use them — added with the modal layer at
  50–52). Free functions, not a trait, since the passive layer dispatches into no
  overridable virtual.
- **Deferred + breadcrumbed (NOT stubbed):** `execute()` (the nested modal loop →
  D9 `OpenModal`), `trackMouse`/`trackKey`/`nextItem`/`prevItem` (modal nav),
  `findItem`/`findAltShortcut`, `do_a_select`/`newSubView`/`mouseInOwner`/
  `mouseInMenus`/`topMenu`, `getItemRect`/`draw`/`getPalette` (`cpMenuView`),
  `getHelpCtx`, streaming (D12). The activation branches of `handle_event`
  (`evMouseDown`, `cmMenu`, alt-shortcut) are breadcrumbed (leave the event live).
- **Verification (no snapshot — nothing draws):** 8 unit tests on `hot_key` (submenu
  recursion, disabled-skip bite, separator/no-key, submenu-own-key-no-match) +
  `update_menu_commands` (recursive regray, negated-predicate bite, submenu-flag
  untouched); **2 integration tests** through real `pump_once` — a `#[cfg(test)]
  MenuProbe` (FakeList precedent) proving the broker end-to-end (enable→regray→enabled,
  disable→`cmCommandSetChanged`→request→apply→disabled, bite-checked) + the
  accelerator-post path (enabled posts, regrayed-disabled posts nothing).

### NEXT — Row 50 `TMenuBar` / 51 `TMenuBox` / 52 `TMenuPopup` (the menu modal layer)
This is where the deferred `execute()` and the **D9 view-triggered async-modal path**
(`Deferred::OpenModal(Box<dyn View>)` + a posted completion `Command`, guide D9
"exec_view — corrected") get **built** — both `execView` calls in the C++
(`do_a_select`→`owner->execView(this)` and `execute`'s submenu
`owner->execView(target)`) are view-triggered modals needing this path. Each menu
view also needs `getItemRect` + `draw` (so `execute()`'s navigation is testable). At
that point introduce the `MenuView` trait + the `current`/`parentMenu` fields (the
row-28 `ListViewer` trait + free-fn-over-state shape), and port the modal navigation
(`trackMouse`/`trackKey`/`nextItem`/`prevItem` — note the subtle separator-skipping +
`prevItem`-via-`nextItem`). `TMenuBar` (50) = horizontal layout; `TMenuBox` (51) =
vertical popup box w/ frame glyphs (D7); `TMenuPopup` (52) = `popupMenu()` free fn +
exec. **Initial-regray gap to close at 50/51:** the C++ `TMenuItem` ctor reads
`commandEnabled` at construction, but our row-46 builder has no command set, so
menus are born **all-enabled** and the row-49 broker only corrects them on a
`cmCommandSetChanged` broadcast — which does **not** fire at startup
(`default_command_set` seeds directly, no toggle). So a menu with a startup-disabled
command (`cmZoom`/`cmClose`/`cmResize`/`cmNext`/`cmPrev`) would *draw* enabled until
the first broadcast. Trigger an initial `Deferred::UpdateMenu` on menu-bar insert
(or have `Program` broadcast `cmCommandSetChanged` once at startup) so the first
paint is correct. Wiring a real menu bar into `Program` is the first emitter of
`cmTile`/`cmCascade`/`cmDosShell` → then wire the row-32 breadcrumb in
`program_handle_event` + build `Desktop::tile`/`cascade` geometry (see the row-32
breadcrumb section below).

## Prior session — Row 46 `TMenu` data tree (FOUNDATION)
First Phase-4 row: the **menu data tree** (`TMenuItem`/`TSubMenu`/`TMenu`,
`menus.h`/`menu.cpp`) — pure data + a builder, **no `View`** (that's row 49).
`src/menu/mod.rs`, wired into `lib.rs` (`pub use menu::{Menu, MenuBuilder,
MenuItem}`). Built main-thread/Opus-orchestrated: brief
(`docs/briefs/row46-menu-data-tree.md`, advisor-vetted design) → Opus implementer
→ **full two-stage review** (spec then quality, fresh C++-adversarial Opus agents,
both PASS) → doc-only fixes → integrate.

- **Data model = a 3-variant enum** (`MenuItem::{Separator, Command{…},
  SubMenu{…}}`), the type-safe translation of the C++ `union { param; subMenu }`
  discriminated by `name==0`⇒separator / `command==0`⇒submenu / else command.
  Shared fields (`name`/`key_code`/`help_ctx`/`disabled`) read via or-patterns;
  **no speculative common sub-struct** (advisor: add it later iff 49–52 want it).
  `MenuItem::disabled_mut() -> Option<&mut bool>` (None for `Separator`) for the
  row-49 command-graying loop.
- **`Menu { items: Vec<MenuItem>, default: Option<usize> }`** — C++ linked list
  `next` → `Vec`; `deflt` pointer → an **index**. The builder sets `default =
  Some(0)` on first push (C++ `TMenu(itemList)` head, no separator-skip), `None`
  when empty; both fields are `pub` and the two-arg C++ `TMenu(itemList, deflt)`
  allows a non-head default, so `default` is documented as *any valid index*.
- **`key_code: Option<KeyEvent>`** (None == C++ `kbNoKey`, faithful to the
  decomposed key model = absence of a key event); **`param: Option<String>`**
  (None == C++ `param==0`; empty `""` → `None`).
- **Builder replaces C++ `operator+`** (`MenuBuilder`: `.separator()`,
  `.command(name,cmd)`, `.command_key(name,cmd,key,param)`,
  `.submenu(name,key,|m| …)` closure-nested, `.item(MenuItem)` raw escape hatch).
  Local `fn alt(char) -> KeyEvent` convenience (mirrors `kbAltX`; `key.rs`
  untouched).
- **Verification is NOT a snapshot** (pure data, renders nothing): the lead test
  builds the canonical File/Window menu via the builder and `assert_eq!`s it
  node-for-node against a hand-built literal tree (a *different* code path, so a
  builder bug can't pass silently) + 5 edge-case tests. **6 tests, all pass.**
- **Scope fenced (FOUNDATION-creep guard):** no `View`/draw/event/`execute`/
  `findItem`/`hotKey`/`getItemRect`/streaming — all rows 49–52.

## Prior session — Row 32 `TApplication` (`3e6645f`, MECHANICAL)
The thin D2 embed wrapper over `Program` (row 31): `Application { program: Program }`,
the type a real app constructs. **Genuinely thin by dependency order**
(advisor-confirmed) — all of `TApplication`'s substance is deferred, so the row is
the type + one real body + faithful breadcrumbs, deliberately NOT padded. Built
main-thread/Opus-orchestrated: tight brief (`docs/briefs/row32-tapplication.md`) →
Sonnet implementer (in a `/scratch` worktree) → spec review (fresh C++-adversarial
agent) → fixes → integrate.

- **`Application`** forwards `run`/`pump_once`/`exec_view`/`desktop`/`end_modal`/
  `end_state`/`{enable,disable,command_enabled}_command` + `program()`/`program_mut()`
  escape hatches — hand-written one-liners. **(Note: `#[delegate]` does NOT apply
  here** even though it later landed and was adopted everywhere — that macro
  generates the `View`-trait forwarding impl for D2 embeds; `Application` forwards
  `Program`'s *inherent* loop methods, not the `View` trait, so it stays
  hand-written. It is correct as-is.)
- **`get_tile_rect()` is the one real body** → new **`Program::get_tile_rect`**
  (the desktop child's extent = `deskTop->getExtent()`, local-origin `(0,0,w,h)`,
  `None` if no desktop; `&mut self` because `Group::find_mut` is `&mut`). Placed on
  `Program` (not `Application`) because `Application` can't reach the private `group`,
  and the future command handler — also in `Program` — reuses it.
- **Deferred (NO dead stubs — omit-until-consumer, the row-35/48 rule):**
  `tile`/`cascade` (need `Desktop::tile`/`cascade` geometry [`mostEqualDivisors`/
  `iSqr`/`calcTileRect`/`dividerLoc`/`doCascade`, `tdesktop.cpp`] + a menu to emit
  cmTile/cmCascade + a way to test → Phase 4); `dosShell`/`suspend`/`resume` (need a
  backend terminal-suspend seam + SIGTSTP); `initHistory`/`doneHistory` (history
  subsystem unported); `TAppInit` subsystem init **dropped** (subsumed by the
  `Backend`/`Renderer` construction path).
- **Command handling breadcrumbed, not wired:** `TApplication::handleEvent`'s
  cmTile/cmCascade/cmDosShell are **program-level** → a TODO in `program_handle_event`
  **after** `group.handle_event` (faithful: C++ runs `TProgram::handleEvent` first),
  beside the QUIT catch. Blocked on the deferred bodies. The consts
  `Command::{TILE,CASCADE,DOS_SHELL}` already exist + are enabled in
  `default_command_set`, but **nothing emits them yet (no menus)** — Phase 4 menus
  are the first emitters; when they land, wire this breadcrumb + build the desktop
  geometry together (trigger + body + test in one go).
- **Review caught + fixed a BLOCKER:** the implementer first added empty
  `tile`/`cascade`/`dos_shell` methods on `Application` — dead stubs (the planned
  handler is in `program_handle_event`, which can't reach `Application`); deleted,
  deferral kept in docs + the breadcrumb. Plus 2 MINORs fixed: breadcrumb moved
  post-dispatch; the `get_tile_rect` test made discriminating (inset 80×20 desktop on
  an 80×25 backend pins desktop-extent vs screen-extent — a screen-rect impl fails it).

## Also landed — the `#[delegate]` proc-macro (`47894f0`…`66ab55f`)
The D2 embed-and-delegate pattern (`Wrapper { inner: Inner }` re-implementing the
whole `View` trait by forwarding to `inner`) was hand-written boilerplate in every
wrapper (Dialog→Window, the cluster family, etc.). It is now a proc-macro:
**`#[delegate(to = <field>)]`** in the new **`tvision-macros`** crate (a workspace
member; the repo root is now a Cargo workspace `["tvision-macros"]`). Applied to a
struct, it generates the `View`-trait forwarding `impl` to the named field.

- **Adopted codebase-wide**, replacing the hand-rolled forwards and the
  `cluster_wrapper!` macro: `cluster` (`2a715a0`), `Window` (`c357c3a`, `to=group`),
  `Dialog` (`e4eaad3`, `to=window`), `ParamText` + `Label` (`be70841`), `Desktop`
  (`7e90907`, `to=group`), and the `hello` example's `AboutDialog` (`415edb8`,
  `to=dialog`).
- **Spec + test:** a "full `View` forwarder spec" with a behavioral spy test
  (`4d92646`) → new integration test **`tests/delegate_view.rs`** (the +2 in the
  build-state count); code-review fixes for docs/diagnostics/drift-signposts
  (`375ef03`); a design note + a CLAUDE.md convention (`30cfe1f`).
- **Implication for future D2 wrappers:** prefer `#[delegate(to = inner)]` over
  hand-writing the `View` forwards. It applies when the wrapper forwards the **`View`
  trait** to an embedded `View` field; it does NOT apply to inherent-method forwards
  (e.g. `Application`→`Program` loop methods). When you override a method (the
  wrapper's own `handle_event`/`valid`), keep that method and let the macro forward
  the rest — check the macro's drift-signpost docs for the override pattern.

### Prior session — Row 48 `TListBox` (`fc66637`, MECHANICAL)
The first **concrete** `TListViewer`, proving the row-28 trait seam end to end.
Built main-thread/Opus-orchestrated: tight brief
(`docs/briefs/row48-tlistbox.md`) → Sonnet implementer → full two-stage review
(SPEC then QUALITY, fresh C++-adversarial Opus agents) → integrate.

`ListBox { lv: ListViewerState, items: Vec<String> }` (`src/widgets/list_box.rs`)
reuses **all** of `TListViewer`'s draw/event/nav verbatim via the `ListViewer`
trait, overriding **only `get_text`** (`items.get(item as usize).cloned().
unwrap_or_default()` — collapses the C++ `items==0→EOS` + OOB cases, panic-free);
`is_selected`/`select_item` **inherit the base** (C++ overrides neither). `impl
View` delegates `draw`/`handle_event`/`set_state`/`cursor_request`/
`apply_list_scroll`/`as_any_mut` to the `list_viewer::*` free fns (the `FakeList`
template). Wired into `widgets/mod.rs` + `lib.rs`.

- **D10 value protocol — first consumer beyond `TInputLine`:** `value() →
  FieldValue::Int(focused)` (the `getData` selection half; the collection is
  config `new_list` manages, NOT part of the transferable value — no `List`
  variant, `FieldValue` grows per consumer).
- **`set_value` DEFERRED** (advisor-confirmed): the **`Context`-free**
  `View::set_value` signature can't republish the v-bar (C++ `setData` =
  `newList`+`focusItem`, both need a `Context` in our model), so a partial would
  leave the scroll thumb desynced after a scatter. Lands with the **dialog
  gather/scatter** consumer (inputBox/Batch E), which must itself solve threading
  a `Context` into scatter. `TODO(set_value: dialog gather/scatter)`.
- **Population is post-insert** (the ctor has no `Context`): `new_list(items,
  ctx)` (`set_range` + `focus_item(0)` iff `range>0`) **plus**
  `list_viewer::update_steps(ctx)` for the page/arrow steps — miss either and the
  thumb starts unsynced. Documented on the type.
- **Dropped:** `dataSize`/`TListBoxRec` (→ typed value), streaming (D12),
  `drawView` (D8). The dialog gather/scatter group-walk stays deferred (no
  consumer yet).
- **Process catch — out-of-scope creep reverted:** the implementer also added an
  exported `delegate_view_rest!` macro to `src/view/view.rs` + refactored
  `examples/hello.rs` to use it — unrelated to row 48, unreviewed (both review
  agents were scoped to `list_box.rs`), and touching a FOUNDATION file. Reverted
  before commit. The macro is a genuinely useful D2-embed helper; if wanted, do it
  deliberately as its own reviewed change.

### Prior session — Row 28 `TListViewer` (`c1ad789`, FOUNDATION)
`TListViewer` (base for `TListBox` 48, history, color/file lists) drives two
sibling scrollbars like `TScroller` but **diverges structurally in two ways** the
"reuse the broker verbatim" line glossed over — both confirmed with the advisor
*before* building. Built main-thread/Opus: brief → Opus implementer → two-stage
review (SPEC then QUALITY, fresh C++-adversarial agents) → fixes. Brief:
`docs/briefs/row28-tlistviewer.md`.

**Divergence 1 — `ListViewer` is a TRAIT, not a concrete struct (the `Validator`
pattern, NOT the `Scroller` embed shape).** `TListBox` reuses `TListViewer::draw`
while *overriding* the virtuals `getText`/`isSelected`; a D2 concrete-embed base
physically cannot dispatch back into the embedder's `getText` from the base's own
`draw`. So:
- `ListViewer: View` trait — `lv()`/`lv_mut() -> &ListViewerState` accessor +
  defaulted `get_text`/`is_selected`/`select_item`.
- `ListViewerState` struct holds the data members (`state: ViewState`, `num_cols`,
  `top_item`, `focused`, `range`, `indent`, `h_scroll_bar`/`v_scroll_bar` ids).
- The shared draw/event/nav logic lives as **free functions generic over
  `<L: ListViewer + ?Sized>`** (`list_viewer::draw`/`handle_event`/`focus_item`/
  `focus_item_num`/`set_range`/`update_steps`/`apply_scroll`/`set_state`/
  `focused_cursor`), which a concrete widget's `View` impl calls.
- Object-safety: `ListViewer` is **not** object-safe (`get_text -> String`) — fine,
  it's only ever a generic bound; concrete widgets are still `Box<dyn View>`.
- A `#[cfg(test)] FakeList` (Vec-backed) is the first consumer (a real consumer for
  the draw/nav tests, NOT a dead stub). **Row-48 `TListBox` is the production one.**

**Divergence 2 — the read-sync WRITES BACK (the scroller never did).** C++
`focusItem → vScrollBar->setValue(item)`; in our model the read-sync issues a
deferred `ScrollBarSetParams{value}`. New mechanism, **scroller path untouched**:
- New defaulted-no-op **`View::apply_list_scroll(&mut self, h, v, ctx)`** + new
  **`Deferred::SyncListViewer{list,h,v}`** + a pump apply arm that calls the **trait
  method (NO downcast** — you can't cast `dyn View → dyn ListViewer`, unlike the
  scroller's `as_any_mut` downcast to a single concrete type).
- **TERMINATION (the centerpiece property):** the vbar→sync→setValue cycle
  terminates **only because `ScrollBar::set_params` is change-guarded**
  (`scrollbar.rs:219/224` — broadcasts `SCROLL_BAR_CHANGED` iff `old_value !=
  a_value`), so the write-back of the already-current value is a silent no-op.
  Proven by a discriminating termination test through real `pump_once` drains
  (6 passes asserting quiescence; bite-checked — removing the guard makes it spin).
- **`indent` cached** on `ListViewerState`: draw can't read the sibling hbar live,
  so the hbar `value` is cached and refreshed by the same sync (the hbar
  `cmScrollBarChanged` branch, C++ "just drawView", becomes "update the cache").

**Reused verbatim from row 27:** `Deferred::ScrollBarSetParams` (setRange +
ctor-setStep) and `SetVisible` (setState show/hide), `Broadcast{source}` as the
`source ∈ {h,v}` filter, `View::value() → FieldValue::Int`.
- **`setState`** uses the C++ **`active && visible` AND-condition** for show/hide
  (NOT the scroller's `active || selected` — a spec-review crosshair).
- **`cmScrollBarClicked` from an own bar → `select()`** → `ctx.request_focus(id)`
  (the row-41 `Deferred::FocusById` seam).
- **Theme reconciled** to the 5-entry cpListViewer palette (`Active/Inactive/
  Focused/Selected/Divider`) → roles `ListNormalActive`/`ListNormalInactive`/
  `ListFocused`/`ListSelected`/`ListDivider` (the old guessed `ListNormal`/
  `ListSelectedFocused` were unused; provisional colours, `TODO(window-scheme
  remap)`).
- **Deferred + breadcrumbed:** mouse press-and-hold/auto-scroll `do…while
  (mouseEvent)` loop (`TODO(row 31, D9)`; ship single-shot + double-click select);
  `changeBounds` step republish (`TODO(resize)` — **note the distinct formula**:
  C++ `changeBounds` uses vbar plain `size.y` + **both bars preserve arStep**,
  unlike the ctor's `update_steps`; do NOT call `update_steps` for resize —
  corrected in-doc after a spec catch); `showMarkers` + streaming dropped (D8/D12);
  scroller/listviewer read-sync unification noted optional/out-of-scope.

### Prior session — Row 27 `TScroller` (`543b2c8`, FOUNDATION)
Established THE cross-view scrollbar broker (pump brokers all scroller↔scrollbar
reads/writes at deferred-apply via `group.find_mut(id)` + `as_any_mut`/
`View::value()`; `Broadcast{source}` is the filter, value NOT stuffed into the
message). New `Deferred`: `SyncScrollerDelta` (read → `apply_delta`),
`ScrollBarSetParams` (write, per-field `Option`=preserve), `SetVisible`. New seams
`FieldValue::Int` + `ScrollBar::value()`. Dropped (D8) `drawLock`/`drawFlag`/
`checkDraw`/`drawView`. `Role::ScrollerSelected` + `changeBounds` resize-republish
deferred to `TEditor` 66. Brief: `docs/briefs/row27-tscroller.md`.

## What landed the PRIOR session (validator wave, `43e5c68`)
The full row-35→39 wave + the **D10 typed-value protocol**, built as one Opus
implementer + full two-stage review (SPEC then QUALITY, fresh C++-adversarial
agents). Brief: `docs/briefs/row35-39-validator-inputline.md`.

- **TValidator (35)** → `src/validate.rs`: object-safe abstract `Validator` trait
  (D2) — `is_valid_input(&self,&mut String,bool)` / `is_valid(&self,&str)` /
  `error` / `is_status_ok` (all defaults accept) + provided non-virtual
  `validate`. **`transfer` deliberately omitted** (PORT-ORDER row 35 lists it, but
  it has no overrider until TRangeValidator row 59 → would be a dead stub; the
  row-34 "no dead stubs" rule wins). `tv::Validator`.
- **D10 value protocol** → `src/data.rs`: **`FieldValue`** typed-transfer currency
  — one `Text(String)` variant, **grows per control** (Role/Glyphs convention;
  `Bits(u32)` for cluster + `Int` for range land when those wire their value).
  Defaulted **`View::value(&self)->Option<FieldValue>` / `set_value(&mut self,
  FieldValue)`** (the getData/setData successors). The dialog **gather/scatter
  group-walk is DEFERRED** to its first consumer (inputBox / Batch E) —
  breadcrumbed in `data.rs`.
- **TInputLine (39)** → `src/widgets/input_line.rs`: faithful `tinputli.cpp` port.
  Draw (scrolled `moveStr` + ◄/► arrows + selection redraw + cursor), full
  keyboard (nav / word-nav / edit / Ins-toggle / Shift-block-extend /
  printable-insert with the `maxLen && maxWidth && maxChars` guard / Ctrl-Y),
  single-shot mouse positioning **+ the faithful single edge-click scroll-by-one**,
  validator `save_state`/`restore_state`/`check_valid`, `valid(cmd)` (faithful
  return), `set_state`→`select_all`, `value`/`set_value`.
  **Key correction the implementer caught:** `first_pos` is a display **COLUMN**,
  not a byte offset (the brief mis-stated it; `cur_pos`/`sel_*`/`anchor` ARE byte
  offsets). All `data` indexing steps through grapheme helpers — **D13
  panic-safe** (multi-byte tests over `ä€中` BITE).
- **New seams:** `text::prev` (`TText::prev`), `DrawCtx::put_str_part` (`moveStr`'s
  `begin` column-skip), 3 theme roles `Input{Normal,Selected,Arrow}` (provisional
  gray, `TODO(row 34 gray theming)`) + 2 glyphs (◄ U+25C4 / ► U+25BA), `cmValid`,
  `State::cursor_ins`.
- **End-to-end veto test (`8ea87cb`, advisor-flagged):** the headline
  `InputLine::valid()` behavior — a modal must NOT close on OK while a child's
  validator rejects — lived only in isolated widget tests. The actual veto is in
  `exec_view`'s outer `while !valid(end_state)` loop. New integration test in
  `program.rs`: a `Dialog` + `InputLine` + `RejectAll` validator, driven through
  `exec_view` with pre-queued `[cmOK, cmCancel]`, asserts the result is **cmCancel**
  (cmOK vetoed, modal stayed open) + the `ModalFrame` popped. Bite-verified; **no
  bug in the veto path** (`exec_view` honors `valid()` correctly). The `[OK,
  CANCEL]` shape is deliberate — `[OK]` alone loops forever (a permanently-rejecting
  field can never close, which IS faithful). + a `#[cfg(test)] Dialog::insert_child`
  hook.

### Deferred + breadcrumbed in the validator wave (prior session; grep the TODOs)
- **clipboard** cmCut/cmCopy/cmPaste — no `Context` clipboard seam (backend has
  set/get_clipboard; not surfaced to views). `TODO(clipboard)` in `input_line.rs`.
- **command-graying** `updateCommands`/`canUpdateCommands` (enable/disable cmCut/
  Copy/Paste) — needs the `Context` command-set query that **TButton also
  deferred**. `TODO(button/inputline: command-set query …)`. **Menus (Phase 4)
  force this** — add a read-only command-set accessor to `Context` then.
- **mouse press-and-hold / drag-select loops** — `TODO(row 31, D9)`; single-shot
  positioning + the single edge-click scroll only.
- **`valid()`'s `select()` focus side-effect** — C++ focuses the invalid field
  before returning false; needs `&mut Context` + the **focus-by-ViewId** seam
  (`Deferred::FocusById` / `request_focus`, already built at row 41).
  `TODO(valid-select)`. The **return value is faithful** (gates modal OK).
- **validator `transfer` hook** — `TODO(row 59)` at both `value`/`set_value`
  sites; TRangeValidator will produce a typed non-`Text` value (→ `Int`).
- **`Validator::error`→msgbox** — `TODO(msgbox row 63)`.
- **`cur_pos` re-clamp hazard** — `TODO(row 59/62)`: a future *mutating* validator
  that SHRINKS `data` could leave `cur_pos` past EOS / mid-grapheme → D13 panic.
  Unreachable now (abstract validator never mutates); re-clamp when the first
  auto-fill validator (Range/PXPicture) lands.

## NEXT — follow PORT-ORDER in sequence

Lowest-numbered incomplete rows = the work. Next up:

### Phase-4 breadcrumb from Row 32 `TApplication` (`3e6645f`, done a prior session)
When menus emit cmTile/cmCascade/cmDosShell, the deferred bodies land
**together** — build
`Desktop::tile`/`cascade` geometry (`mostEqualDivisors`/`iSqr`/`calcTileRect`/
`dividerLoc`/`doCascade`, `tdesktop.cpp`) + wire the breadcrumb in
`program_handle_event` (after `group.handle_event`, beside the QUIT catch, calling
`desktop.tile/cascade(get_tile_rect())`) + test it with real tileable windows in
one change. `dosShell` separately needs a backend terminal-suspend seam + SIGTSTP.

### Phase 4 — the immediate next work, in PORT-ORDER order
- **Phase 4 — menus + status line** (the path to a fully drivable app):

  **Menus:** `TMenuItem`/`TSubMenu`/`TMenu` (46, FOUNDATION — the menu data tree;
  C++ `operator+` builders → a Rust builder API) **✅ DONE this session**
  (`src/menu/mod.rs`) → **NEXT: `TMenuView` (49, FOUNDATION** — hotkey/shortcut
  dispatch, the `evBroadcast` mask; consumes the row-46 tree, `current` = an index
  into `Menu::items`, command-graying via `MenuItem::disabled_mut`) → `TMenuBar`
  (50) / `TMenuBox` (51) / `TMenuPopup` (52, popup exec via D9). **Menus force the deferred
  `Context` command-set query** (command graying) — build that read-only accessor
  on `Context` when you hit it (additive; the deferred-effects refactor stabilized
  `Context::new` for *effects*, a read accessor is a separate additive concern).
- **Status line:** `TStatusItem`/`TStatusDef` (47) → `TStatusLine` (53,
  FOUNDATION — hint()/help-ctx→hint mapping).
- Wiring menus + status line into `Program` lets the `examples/hello.rs` demo grow
  a real menu bar + status line (and shifts the desktop down — revisit the
  `ModalFrame`/`DragCapture` "(0,0)-desktop absolute-coords" caveats then).

### Available parallel fan-out (efficiency, not a competing direction) — Batch C: concrete validators (58–62, MECHANICAL)
Fully unblocked by `TValidator` (35); **fully parallel among themselves** → the
clean worktree fan-out cadence (Sonnet implementers, `isolation:"worktree"`,
orchestrator integrates + pre-seeds any shared files). These are PORT-ORDER's
"Parallelizable batches" — run them concurrently whenever convenient; they don't
displace the in-sequence FOUNDATION work above. C++ all in `tvalidat.cpp`:
- **58 `TFilterValidator`** (char allow-list), **59 `TRangeValidator`** (int range;
  **resolves the deferred `transfer` hook + the `cur_pos` re-clamp hazard** above —
  and now has `FieldValue::Int` ready [added by row 27]; so this one is
  FOUNDATION-ish, do it carefully),
  **60 `TLookupValidator`** (abstract lookup), **61 `TStringLookupValidator`**,
  **62 `TPXPictureValidator`** (Paradox picture-mask state machine — the big one;
  `picture()`/`process()`/`scan()`/`group()`/`iteration()` — sets `status=vsSyntax`,
  which is what `is_status_ok()` and TInputLine `valid(cmValid)` already consult).
Each validator's `is_valid_input` may **mutate** `s` (auto-fill) — that's the
trigger for the TInputLine `cur_pos` re-clamp `TODO(row 59/62)`.

### Then `msgbox` (63) + Batch E fan out
`messageBox`/`inputBox` (`msgbox.cpp`) is buildable now (TButton + TStaticText +
TInputLine exist) but is the **first consumer of the D9 view-triggered async-modal
path** (`Deferred::OpenModal` + posted completion `Command`) — guide D9 "exec_view
— corrected" carries that design; build when a menu/msgbox needs it (Phase 4), not
before. Batch E dialog families (color/file/chdir/editor/outline/textview) fan out
once their leaf prereqs exist.

## Standing process reminders
- **Fan-out cadence is for gap-free MECHANICAL leaves only** (parallel worktree
  implementers, `isolation:"worktree"`, Sonnet, orchestrator integrates shared
  `mod.rs`/`lib.rs` + pre-seeds `theme.rs`). **FOUNDATION rows → per-row, Opus,
  full two-stage review.** Commit completed rows before dispatching worktree
  agents that build on them (worktree branches from the last *commit*).
  **Worktree location:** `isolation:"worktree"` now lands under
  `/scratch/oetiker/claude-worktrees/` via the `WorktreeCreate` hook — but only
  after a session **restart** (hooks load at startup). Before that, isolation goes
  to the project's `.claude/worktrees/`; create the worktree manually at the
  `/scratch` path + dispatch a **non-isolated** subagent instead (the row-32
  cadence). Verify where a probe worktree actually lands before relying on it.
- **Two-stage review stays mandatory** (SPEC then QUALITY, fresh C++-adversarial
  agents against the **C++ + guide, NOT the brief** — the brief can be wrong, as
  the validator wave's `first_pos` mis-statement proved). Make round-trip/unit tests
  **discriminating + bite-checked** (verify a finding fails before/passes after).
  Both stages keep earning their keep: at row 27, **spec** review caught an invented
  active/selected `draw` branch (the base inherits `TView::draw`'s uniform fill) and
  **quality** caught `std::any`-vs-`core::any` + a stale doc; in the validator wave,
  quality caught the untested validator reject/restore path and spec caught a dropped
  double-click scroll.
- **Snapshot workflow** (Appendix B step 4): `cargo-insta` is NOT installed →
  generate a `.snap` with `INSTA_UPDATE=always cargo test <name>`, verify by hand,
  re-run plain, commit the `.snap`.
- Keep per-row briefs **tight + self-contained + inline** (over-long briefs crashed
  a Sonnet implementer's context earlier in Batch B).

## Older standing deferrals (still open, grep the code)
- **`Context` command-set query** (command-graying) — TButton + TInputLine both
  wait on it; **Phase-4 menus force it**.
- **phase signal on `Context`** (plain-letter postProcess accelerator) — 3 waiting
  consumers: button, label, cluster (`is_plain_hotkey` exists but is ungated).
- **`Group::remove` release-after-remove ordering** — a removed selectable child
  never gets `RELEASED_FOCUS{source}`; a `TLabel` whose link is removed at runtime
  keeps a stale `light`. C++ `hide()`s before `removeView`. No consumer hits it yet.
- **`cmResize` keyboard sub-mode** (`window.rs`); **scrollbar auto-repeat +
  thumb-drag** + **cluster drag-cursor** (`TODO(row 31, D9)`); **close
  press-and-hold confirm** (`frame.rs`); **sibling tee-walk** (`framelin.cpp`);
  **shadow casting** (`group.rs`); **gray multi-scheme theming**
  (`TODO(row 34 gray theming)` — realign provisional `*` colours, incl. the 3 new
  Input roles); **row-9 glyphs** continue per-widget.
- **ctrlToArrow / accelerator TODOs** in cluster/scrollbar — shared key helpers
  EXIST (`b53c618`); retire opportunistically.
