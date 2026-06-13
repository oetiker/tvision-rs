# Turbo Vision → Rust: Porting Guide

> The deviation reference for **rstv**, an idiomatic Rust port of
> [magiblot/tvision](https://github.com/magiblot/tvision).
>
> **We port faithfully from the magiblot C++ source.** Class structure, method
> names, control flow, algorithms, and behavior are reproduced as-is unless this
> guide says otherwise. So this document does **not** describe how Turbo Vision
> works — it describes only the places where a faithful, line-by-line port is
> impossible or unwise in Rust, and for each one it specifies the deviation and
> **how the deviated part stitches back into the otherwise-direct port.**
>
> If something isn't listed here, port it straight from the C++.
>
> The dependency-ordered port sequence lives in
> [PORT-ORDER.md](file:///home/oetiker/checkouts/rstv/docs/PORT-ORDER.md); the
> mechanical per-class recipe is **Appendix B**. Together they let `MECHANICAL`
> classes be ported with near-zero judgment.

---

## 1. Why we deviate

Almost every deviation traces to one root cause:

> **Turbo Vision's prefixes, bit-packing, raw pointers, and hand-rolled
> machinery exist because the language of 1991 lacked features Rust has built in.
> The port's job is to recognize each such workaround and replace it with the
> real feature** — then keep everything around it faithful.

`TView` → `tv::View` (a namespace it didn't have). `cmOK` → `tv::Command::OK` (a
type-scoped constant). `state & sfFocused` → `state.focused` (a struct field).
`TEvent.what` union → `enum Event` (a sum type). Same idea every time.

Each deviation below is tagged **forced** (Rust literally won't compile the
direct port) or **chosen** (the direct port compiles but a modern construct is
clearly better), and is written as:

- **Baseline** — what the C++ does.
- **Deviation** — what we do instead.
- **Integration** — how the rest of the faithful port plugs into it.

---

## D1 — Names & namespacing · *chosen*

**Baseline.** `T`-prefixed types (`TWindow`); `cm*`/`sf*`/`ev*`/`kb*`/`hc*`
prefixed global constants; a 256-entry `TCommandSet`. The prefixes are manual
namespacing — 1991 C++ had no real namespaces.

**Deviation.**
- Crate published as `rstv`; **house style `tv::`** on every type/constant
  (consumers add `tv = { package = "rstv" }` once, no `use` needed). The path
  *is* the namespace the `T` prefix was faking.
- Drop the `T` prefix (`TView` → `tv::View`). Methods are `snake_case`
  (`handleEvent` → `handle_event`).
- The `cm*`/`hc*` families become **type-scoped associated constants** in
  `SCREAMING_SNAKE_CASE` on open newtypes — recognizable, no `#![allow]`, and
  user-extensible (the value space stays open, unlike an `enum`):

  ```rust
  pub struct Command(&'static str);
  impl Command {
      pub const OK:     Command = Command("tv.ok");
      pub const CANCEL: Command = Command("tv.cancel");
  }
  // tv::Command::OK ; your app: pub const CMD_REFRESH: tv::Command = Command::custom("myapp.refresh");
  ```

  Likewise `tv::HelpCtx::NO_CONTEXT`. The `kb*` family is the *opposite* case
  (see the refinement below): the set of physical keys is fixed, so it is a
  closed `enum Key` (D4), not a newtype. (`sf*`/`of*` are handled by D5.)
  Because a `Command`'s value is open identity rather than a 0–255 space, the
  command set is a `HashSet<Command>`, not a fixed bit array — and the
  program's enable policy stores `curCommandSet` as its complement, a
  **disabled set** (denylist; see below and
  `docs/design/command-enablement.md`).

> **Newtype vs. enum, by extensibility.** D1's rationale for open newtypes is
> *extensibility*; apply it precisely. **Open, app/view-extensible spaces → open
> newtype** (`Command`, `HelpCtx` — apps and third-party views mint their own
> values). **Closed, fixed sets → enum** (`Key` — the set of physical keys is
> fixed; no external code invents a new key). This refines D1's reason, it does
> not contradict it.

> **`Command`/`HelpCtx` identity is a namespaced `&'static str`, not an
> integer.** *(chosen.)* TV's `cm*`/`hc*` are hand-assigned small integers in one
> flat space. We make the value a namespaced static string —
> `Command("tv.ok")`, exposed via SCREAMING_SNAKE assoc consts plus
> `Command::custom("ns.name")` for external code (same for `hc*`). Safe and
> better because the integers existed only for serialization (`TStreamable`,
> dropped — D12) and for a 256-bit `TCommandSet` (already a `HashSet`); a
> command's value is now pure internal identity. Namespacing makes the
> decentralized constants (below) collision-safe *by construction*, where integer
> ranges only papered over collisions. The command **bus** itself — decoupled
> token dispatch, enable/disable sets, menu/status binding — is good architecture
> and is kept; only the token's representation modernizes. Zero porting cost at
> call sites (`match cmd { Command::OK => … }`, menu tables, events all read the
> same). *Consequence:* TV's "commands ≥ 256 are always enabled / the 256-entry
> trackable range" rule is **subsumed** (it was a bit-array capacity artifact).
> `TCommandSet` becomes a plain, polarity-neutral set over `HashSet<Command>`
> (`enable`/`disable`/`has`/union/intersection/difference) with no range guard
> and no `all()`; the *enabled-by-default* policy lives in `Program`, which
> stores `curCommandSet` as its complement — a **disabled set** (denylist):
> `command_enabled(cmd) == !disabled.has(cmd)`, seeded with exactly the five
> startup-disabled window commands from `initCommands()`
> (`cmZoom`/`cmClose`/`cmResize`/`cmNext`/`cmPrev`). Every command — including
> any app-minted `Command::custom` — is enabled by default *and* maskable,
> which reproduces observable C++ behavior while being strictly more capable
> than the ">255 never maskable" half. Views read it via the per-pump
> `Context::command_enabled` snapshot. Full rationale (including the
> allowlist mistake this replaced): `docs/design/command-enablement.md`.

> **Constants live with their owner.** The `command` module hosts only the
> framework's **shared vocabulary** — the core/dialog/edit/window/app/broadcast
> commands the core generates or interprets generically — and **documents the
> namespace convention**. **View-specific** commands (editor movement/edit,
> file-dialog results, …) are defined *with their view's module* when that row
> lands, exactly as an external third-party view defines its own under its own
> namespace. No privileged central registry.

**Integration.** A regular, mechanical transform — `TFoo → tv::Foo`,
`cmFoo → tv::Command::FOO`. Appendix A is the lookup table. Every site that
*uses* a command/key/help-context ports faithfully; only the spelling changes.

---

## D2 — Inheritance → trait + composition · *forced*

**Baseline.** A deep class tree (`TObject → TView → TGroup → TWindow → …`) with
overridable virtual methods and the "call base, then extend" idiom. Other
subsystems are class hierarchies too (the `TValidator` family, collections).

**Deviation.** Rust has no inheritance, so split what inheritance bundled:

| inheritance gave…           | Rust replacement                              |
| --------------------------- | --------------------------------------------- |
| shared **data members**     | a `ViewState` struct, held by **composition** |
| overridable **virtuals**    | a `View` **trait** with **default methods**   |

```rust
pub trait View {
    fn state(&self) -> &ViewState;
    fn state_mut(&mut self) -> &mut ViewState;
    fn draw(&mut self, ctx: &mut DrawCtx);                 // must override
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) { /* base */ }
    fn valid(&self, _cmd: Command) -> bool { true }
    // …
}
```

The template-method pattern (a base method calling *down* into an override)
works natively: a **trait default method's `self.draw()` dispatches to the
implementor's override** — no hand-built vtable. "Subclassing `TWindow`" becomes
**embed-and-delegate**: hold a `tv::Window`, forward the methods you don't
change, override the ones you do; `self.win.handle_event(..)` is the
"do base then extend" call.

**Integration.** Method names and semantics stay faithful (`draw`,
`handle_event`, `valid`, `data_size`, …). Every *other* class hierarchy ports as
a trait + impls with its protocol unchanged — e.g. `TValidator`'s
`isValidInput`/`isValid`/`transfer` become a `Validator` trait with the same
three hooks. No code generation; an optional `#[derive(DelegateView)]` can later
sugar the user-side embed-and-delegate forwarding.

---

## D3 — Raw pointers → handles + downward context · *forced*

**Baseline.** Bidirectional raw pointers: `owner` (up), a circular
`next`/`prev`/`last` sibling list, `current`/`selected` cross-links; capture
handlers and `message()` receivers hold `TView*`.

**Deviation.** Rust forbids aliased mutable references.
- **Downward ownership is a tree:** a `Group` owns `children: Vec<Box<dyn View>>`
  in Z-order; recursive dispatch (`for c in &mut self.children { … }`) doesn't
  alias.
- **Up/sideways links are `ViewId` handles** (a single global, monotonic id —
  see the resolution-substrate note below), never `&View`. Identity is `ViewId`
  equality.
- **No up-pointers:** the parent passes `&mut Context` / `DrawCtx` *down*,
  carrying everything a child would otherwise reach upward for — including the
  **resolved parent/background style and the current clip rect**.

**Integration.** Focus/selection/traversal *logic* ports faithfully (tab order,
the per-group `current` vs. global `focused` distinction, validate-on-focus-leave,
`makeFirst` raising a window). Only two substitutions: pointer → `ViewId`, and
"reach upward" → "read from `Context`." A targeted `message(view, …)` becomes a
`Context` query addressed by `ViewId` (see D4).

> **Resolution substrate — corrected (was an unexamined default).** A `ViewId`
> is a **single, process-global, monotonic identity** (`NonZeroU64`, keeping the
> `Option<ViewId>` niche), minted once at `insert` and **stamped into the view's
> own `ViewState.id`** so a view knows its own handle. Resolving a `ViewId` to a
> view is the **tree-walk this section always promised**: `View::find_mut(id)` (a
> `Group` searches its children and recurses; a `Group`-embedding view delegates;
> a leaf returns `None`), with `remove_descendant(id, ctx)` for self-removal and
> `ctx.query(id, …)` riding the same walk when a consumer needs it.
>
> This **replaces** the original implementation, in which each `Group` held its
> own generational `ViewArena`, making ids **group-local**. That was never a
> reasoned constraint — it was an artifact of embedding the standalone row-17
> arena inside `Group` (row 26). It had two bad effects: **(a)** it silently
> dropped the global-resolution / `ctx.query` mechanism this section promises,
> forcing each new cross-tree need (drag, close) to invent a bespoke downward
> channel; and **(b)** it carried generational reuse-safety whose validator
> (`is_valid`) was **never called** outside its own unit tests — resolution was
> always `Group::index_of`, which already yields `None` for a removed child, so
> the use-after-free / ABA hazard the generations guarded against could not
> occur. Global monotonic ids preserve the one real benefit (constructing a
> `Group` + children *before* insertion) via a process-wide counter — no
> allocator threading — and a stale handle simply resolves to nothing.
>
> **Unaffected:** genuine D3 *downward* channels (owner extent/size, deferred
> command-enable, deferred capture pushes, owner-data-down to a frame) remain —
> they carry parent→program state a child cannot reach *upward*, which global
> *downward* resolution does not address.

---

## D4 — Events: tagged union + bitmasks → enum + match · *chosen*

**Baseline.** `TEvent { what; union { mouse; keyDown; message; } }`; `what` is a
bitmask; group masks (`evMouse`, `evKeyboard`, `evMessage`) — which even alias
(`evKeyboard == evKeyDown`). `message(receiver, what, command, infoPtr)`
round-trips a result back through the event's `void* infoPtr`.

**Deviation.** A real sum type, matched not masked:

```rust
pub enum Event {
    MouseDown(MouseEvent), MouseUp(MouseEvent), MouseMove(MouseEvent),
    MouseAuto(MouseEvent), KeyDown(KeyEvent),
    Command(Command), Broadcast(Command), Nothing,
}
```

A handled event is consumed by setting it to `Event::Nothing` (the `clearEvent`
equivalent). `message()` splits into two typed mechanisms: a **targeted query**
(a `Context` method routed by `ViewId`, returning `Option<T>` — no `void*`
round-trip) and a **broadcast** (`Event::Broadcast(Command)` pushed into the
queue). The one part of `eventMask` worth keeping — the opt-in for expensive
events (continuous mouse-move, auto-repeat) — survives as a small `EventMask`
bool struct, not a bit-word.

**Integration.** Routing *logic* ports faithfully: positional events to the
top-most child under the cursor; focused events through the three ordered passes
(pre-process → focused → post-process). The phase opt-ins `ofPreProcess` /
`ofPostProcess` are just `options` fields (D5); the rest of `TGroup::handleEvent`
translates directly.

> **The `Key` shape (decomposed, crossterm-shaped).** `TKey`/`kb*`/`KeyDownEvent`
> become a **decomposed** model — a closed `enum Key` (D1):
>
> ```rust
> pub enum Key { Char(char), F(u8), Enter, Esc, Backspace, Tab,
>                Up, Down, Left, Right, Home, End,
>                PageUp, PageDown, Insert, Delete }
> pub struct KeyModifiers { shift: bool, ctrl: bool, alt: bool }   // D5
> pub struct KeyEvent { key: Key, modifiers: KeyModifiers }
> ```
>
> **No modifier-combined variants:** `Ctrl+C` = `Key::Char('c')` + `ctrl`;
> `Shift+Tab` = `Key::Tab` + `shift`; `Alt+F3` = `Key::F(3)` + `alt`. This
> mirrors magiblot's own canonical `TKey`, which already normalizes combined
> codes to `{base key, modifier flags}` (`kbCtrlA == TKey('A', kbCtrlShift)`) and
> checks modifiers via a separate `controlKeyState` channel. `kbNoKey` = the
> *absence* of a key event (no `Null` variant). DOS scan-code/ASCII pairs
> (`kbEnter = 0x1c0d`) do not survive — an enum variant carries no number at all;
> the name transfers, the magic value does not. The `CrosstermBackend` translates
> crossterm's key model into this at a later row.

> **`message()` — corrected (the payload + query were droppable only because the
> id substrate was broken).** D4 originally "dropped `infoPtr`" and deferred the
> targeted query. The whole-tree audit of C++ `message()` (42 call sites) shows
> the objective splits cleanly onto the corrected D3 substrate (global resolvable
> `ViewId` + `find_mut`). **`infoPtr` is polymorphic** — it is used three unrelated
> ways, and only one of them maps onto `source: Option<ViewId>`:
>
> * **39/42 are the broadcast-subject case** — fire-and-forget `message(owner,
>   evBroadcast, cmX, this)`: the return is ignored; the only thing carried is
>   *which view* it is about. These become a posted **`Broadcast { command:
>   Command, source: Option<ViewId> }`** (**built now** — Phase A): the `void*
>   infoPtr` is reinstated as a **resolvable `ViewId`**, not a pointer. A receiver's
>   C++ `infoPtr == hScrollBar` becomes `source == self.h_scroll_bar`. Threaded
>   from each emitter (`source = self.state().id()` = C++ `this`); pump-internal
>   broadcasts about no view (`cmCommandSetChanged`, `cmTimerExpired`) pass `None`.
> * **The command-target case** (`cmZoom`/`cmClose`, `infoPtr = owner`) is **not
>   carried and not needed.** It is *not* a broadcast subject — it is a *target*
>   hint on an `Event::Command`. The frame posts these **only while `sfActive`**
>   (`tframe.cpp` 152/171), so the target is always the *active* window;
>   focused-command routing already delivers each such command only to the
>   desktop's `current` child = the active window; and the queue drains before the
>   next `poll_event`, so the active window cannot change between post and dispatch.
>   The `infoPtr == 0 || == this` guard is therefore **provably vacuous** — adding a
>   target field to `Event::Command` would feed a check that rejects nothing.
> * **The integer-argument case** (`cmSelectWindowNum`, `infoInt = window number`)
>   is **not** served by `source` — a window number is a plain integer, not a
>   `ViewId`. It joins `cmTimerExpired` under "different payload type, own design"
>   (below), realized at 33d via a **direct walk** (the program asks the desktop to
>   select the child whose `number` matches, gated by `canMoveFocus`).
> * **3/42 consume the return** (Alt-N `cmSelectWindowNum`; an app's
>   `cmCanCloseForm` veto-poll inside `valid()`; one test) — a **synchronous
>   "broadcast a question, get back *was it claimed*"**. Every one is
>   **owner-initiated and downward** (the program's own handler, or `valid()`
>   invoked by the owner) — *never* a view re-entering the tree mid-`handle_event`.
>   So they port as one primitive on the tree owner:
>
>   ```rust
>   // inherent on Group (and Program via its root group):
>   fn message(&mut self, target: ViewId, ev: Event /*+ctx*/) -> Option<ViewId> {
>       let v = self.find_mut(target)?;        // D3 substrate
>       v.handle_event(&mut ev, ctx);
>       if ev.is_nothing() { ev.source() } else { None }   // faithful: payload iff consumed
>   }
>   ```
>
> The aliasing rule (`&mut` forbids re-entering a tree mid-mutation) bars exactly
> one pattern — *a view synchronously querying across the tree from inside its own
> `handle_event`* — and the audit shows that pattern **occurs zero times**. Rust's
> borrow rule and TV's actual usage coincide; nothing needs inverting. `message`
> is **not** a `Context` method (a `Context` deliberately holds no tree) — it lives
> on the tree owner, which is the only place a synchronous `message()` is ever
> called from. `query(id, …) -> Option<T>` is the read-only sibling (`find` +
> read). The synchronous `valid(cmd)` aggregate (`Group::valid`) is the same shape
> already in the tree, and `cmCanCloseForm` is an app-specific specialization of it.
> This return-consuming `message()`/`query` primitive is **designed but not built**;
> the sketch above is the design of record. Its first real consumer is **row 34**'s
> dialog `cmCanCloseForm` veto — build it there.
>
> **Not solved by this** (different payload type, own design when needed): the
> `cmTimerExpired` `TTimerId` payload (carries *which timer*) and the
> `cmSelectWindowNum` window number (carries *which window number*, an integer) —
> neither is a `ViewId`, so `Broadcast` `source` does not serve them. Alt-N is
> realized via a direct walk at 33d (see the program/window breadcrumbs).

---

## D5 — Bit-word flags → struct-of-bools + verb-enums · *chosen*

**Baseline.** `state`, `options`, `eventMask`, `growMode`, `dragMode` are
`ushort` bit fields combined with `|` and tested with `&` — packed to save bytes.

**Deviation.** We have the bytes; `|` is also untyped and opaque. Each becomes a
`#[derive(Default)]` struct of bools:

```rust
if view.state().focused { … }          // was: state & sfFocused
```

TV's generic `setState(flag, on)` (which also fires side effects) keeps a
recognizable verb-enum over bool storage:

```rust
view.set_state(StateFlag::Focused, true);   // ≈ setState(sfFocused, true)
```

Most consumer code uses verbs: `button.disable()`, `window.show()`,
`input.focus()`.

**Integration.** Each flag's *meaning* and all logic that reads it port
faithfully; only `a | b` / `x & mask` becomes field access (e.g. three-phase
routing reads `options.pre_process`). The families map: `sf*`→`State`,
`of*`→`Options`, `gf*`→`GrowMode`, `dm*`→`DragMode`, masks→`EventMask`.

---

## D6 — Packed attribute bytes → typed Color/Style · *chosen*

**Baseline.** A cell is a char + an attribute byte; `TColorAttr` packs fg/bg/
style into a word; `TColorDesired` is a tagged union (default / BIOS / RGB /
xterm-256).

**Deviation.** Keep `TColorDesired`'s **four-variant design**, drop the packing:

```rust
pub enum Color { Default, Bios(u8), Indexed(u8), Rgb(u8, u8, u8) }
pub struct Style { fg: Color, bg: Color, modifiers: Modifiers }
```

**Integration.** magiblot's quantization ladder (`RGB → xterm-256 → 16 → BIOS →
default`, the 6×6×6 cube + grayscale ramp + BIOS↔xterm bit-swap) ports **faithfully**
— it's good code — but lives in the `Backend` (D11), not the theme. The "reverse"
attribute and the per-cell "exclude from shadow" marker become `Modifiers`.

> **`TDrawBuffer` consequences (ratified during the row 7 port).** Two follow-on
> *chosen* deviations, because typed attributes have no `0` sentinel: (1)
> `moveChar`'s overload where a `0` char/attribute means "retain what's already
> there" is **dropped** — `move_char` always writes both char and `Style`;
> single-cell retain-edits use `put_char` / `put_attribute`. (2) `moveBuf`, which
> reinterpreted a raw byte buffer as a string, becomes a **typed cell copy**
> (`move_buf(indent, &[Cell])`). The width-clipping/`capacity` logic of
> `moveStr`/`moveCStr`/`moveChar` ports faithfully.

---

## D7 — Palette chains + hardcoded glyphs → Theme · *chosen*

**Baseline.** `getPalette()` returns length-prefixed strings of byte indices that
map through the owner chain up to the application palette; `getColor(idx)` walks
it. Drawing glyphs (frame corners, scrollbar arrows, marks, shadows) are
literals scattered through widget source.

**Deviation.** A theme owns both:

```rust
pub struct Theme { styles: StyleMap /* Role -> Style */, glyphs: Glyphs }
```

A view asks `ctx.theme.style(Role::FrameActive)` etc.; **all** glyphs live in
`Glyphs` (frames incl. tee-connectors `├┤┬┴`; per-surface shadows; scrollbar
arrow sets; check/radio marks; window decorations as composite `&str` like
`"[✕]"`). Resolve *state → role* in one centralized mapper, not per-widget.

**Integration.** Each `getPalette`/`getColor` call site maps to a named `Role`;
the role enum must cover the state variants the widgets need (active/passive/
dragging frames; `*Normal`/`*Focused`/`*Disabled`/`*Pressed`; the
normal/focused/selected/selected-focused list matrix; an error/warning/info/
success family). The default theme reproduces the classic blue look.

**`Role` is a *closed enum* — but that is a port-phase default, not a hard rule.**
Note this does **not** follow the D1 "open newtype for extensible families" rule
(`Command`/`HelpCtx`): those need *open runtime identity* because their values
cross the app↔framework boundary and are routed/dispatched by code that never saw
them. A `Role` is different — it is always resolved at draw time *by the code that
owns it* (`ctx.style(Role::X)` is written by whoever draws `X`), so even an app's
custom widget knows its roles at compile time. What an app would actually need is
not open identity but a *`Theme` extension point* (register new role→`Style`
entries) — a separate mechanism we have **not** built, because nothing in the TV
port (a fixed class set) consumes it. The closed enum is chosen for the *porting*
phase because it buys: a fixed `[Style; ROLE_COUNT]` array with a total,
compile-time-exhaustive `index()` (so `style()` never panics, no `HashMap`, no
default-on-miss), and compiler-guided growth (`match` + `ROLE_COUNT` + `ALL_ROLES`
turn a forgotten role into a build error) during exactly the phase roles churn
fastest. It does **not** restrict theming *refinement* — every `Style` is already
overridable via a custom `Theme`. The only deferred capability is apps minting
*new role names*; when a real app-author audience with custom-themed widgets
needs it, reopen additively (a `Role::Custom(&'static str)` arm backed by an
overflow map — built-ins stay array-fast, customs pay a `HashMap` — or a `Theme`
builder accepting extra entries). Revisit the open-newtype option on its merits
then; do not mistake the closed enum for a permanent boundary.

---

## D8 — Per-write occlusion + damage-tracking → whole-tree redraw + diff · *chosen*

**Baseline.** `TVWrite` computes per-cell visibility *at write time*;
`drawHide`/`drawShow`/`drawUnderView`/`drawUnderRect`, the two-phase
`putInFrontOf`, and the buffered group (`buffer`, `lock`/`unlock`, `ofBuffered`,
`lockFlag`) all exist to minimize *partial* repaints — because in 1991 every cell
you repainted was also a cell you wrote to the terminal.

**Deviation.** Two layers with very different costs:

1. **In-memory redraw (cheap):** every update cycle, paint the whole tree
   back-to-front into a back buffer. RAM only, microseconds.
2. **Terminal flush (diff-bounded):** diff the back buffer against the front
   buffer, emit escape sequences only for changed cells, then swap.

Because the terminal only pays for real change, full redraw is effectively free,
and the entire damage-tracking apparatus becomes unnecessary.

**Integration / consequences.**
- **Occlusion = painter's algorithm** — a later (higher) view overwrites an
  earlier one. No write-time visibility computation.
- **Drop the whole damage-tracking family.** `makeFirst`/`putInFrontOf` keep
  only their `Vec`-reorder role; a z-order change or window move is just *mutate
  the tree, redraw, diff*.
- **Shadows** are cast per view *during* the back-to-front draw, right after a
  view paints its content: darken the cells in its `+2 col, +1 row` region —
  **preserve each cell's glyph, replace only its color** with `Role::*Shadow`,
  guarded by a per-cell "already shadowed" marker. Higher views drawn afterward
  overwrite any shadow they cover, so shadows respect z-order for free.
- **Shadow-over-text needs no special handling.** The lower window's text is
  freshly drawn beneath every frame; the shadow merely recolors those cells.
  There is no "restore when the shadow moves away" problem, because nothing is
  mutated in place — the next frame repaints from scratch. (A single *global*
  post-pass over the shadow rectangles would be **wrong**: it would paint a lower
  window's shadow on top of a higher window that overlaps the region.)
- **Clip rects** remain — but for *correctness* (a view must not paint outside
  its bounds), never for write-minimization. That job is entirely the diff's.

We do not repaint unconditionally — only after an event/timer that might have
changed something — but that's a single app-level flag, not per-view damage rects.

---

## D9 — Nested blocking modal loops → single loop + capture stack · *forced*

**Baseline.** `execView(dialog)` spins its *own* nested `getEvent` loop until the
modal closes; `dragView` and a pressed button's mouse-tracking do the same.

**Deviation.** Rust can't nest a blocking loop that re-borrows the view tree.
- **One** non-recursive event loop.
- A **LIFO stack of capture handlers** that see each event *before* normal
  view-tree routing and may consume or pass it through.
- Modality, drag, and press-tracking are **handlers**, not loops: a modal handler
  that consumes every otherwise-unhandled event *is* the modal loop. Handlers
  hold `ViewId`, not view references.

**Integration.** Animation / cursor-blink / auto-repeat schedule against an
**injected `Clock`** with cancelable handles and set the loop's poll timeout —
never `sleep` (this is also what makes timing deterministic under test, D11).

> **`exec_view` — corrected (the two modal-invocation paths; ratified at row 34).**
> The original note said "`exec_view` can't block-and-return; deliver the result
> by posting a completion `Command`." That is true for *one* of the two callers,
> and the type system draws the line between them cleanly:
>
> * **Owner-/top-level-initiated (sync return, `row 34`).** `exec_view` is an
>   inherent `&mut self` method on **`Program`** (`TGroup::execView`'s owner). It
>   runs a **nested `while end_state.is_none() { pump_once() }`** loop (the
>   `TGroup::execute` shape, incl. the outer `while !valid(end_state)`), then pops
>   its pushed `ModalFrame` and returns the end command **synchronously**. This is
>   **not** a forbidden nested *re-entrant* loop: `pump_once` releases the view-tree
>   `&mut` borrow stack between iterations, and **a `View` holds only `&mut Context`,
>   never `&mut Program`** — so a view *cannot* call `exec_view` from inside its own
>   `handle_event`. The compiler enforces that the sync loop only ever runs at the
>   top level (startup, an app's `main`, a test driving pre-queued events), where
>   no other `pump_once` borrow is live. The D9 "one non-recursive loop" rule is
>   preserved: there is still exactly one `pump_once`; `exec_view` just *drives* it
>   in a bounded loop instead of `run()`.
> * **View-/menu-triggered (async, `Phase 4`).** A menu item or button that wants a
>   modal *is* inside `handle_event`, so it cannot call `exec_view`. It **requests**
>   the modal downward — a `Deferred::OpenModal(Box<dyn View>)` variant (or posts an
>   app command) — and `run()` drains the request **between pumps** and calls
>   `exec_view` itself; the result is delivered back to the requester via a **posted
>   completion `Command`** (the original note's mechanism). *Designed, not built —
>   no menu/button exists until Phase 4; row 34 builds only the sync path.*
>
> **Known deviation — program-level handling runs during modal pumps (row 34).**
> Because our single loop keeps calling `program_handle_event` on every pump,
> the Alt-N window-selection block and the `cmQuit → end_state` catch are live
> *during* a modal. C++ does the opposite: `TGroup::execView` → `p->execute()`
> (`tgroup.cpp:205`) dispatches via the **dialog's** `handleEvent`, so
> `TProgram::handleEvent` (Alt-N + `cmQuit → endModal`, `tprogram.cpp:205`) is
> out of the modal dispatch path. Consequence: here a `cmQuit` during a modal
> ends the modal (with `QUIT`) and Alt-N could reach the desktop under a modal;
> in C++ the dialog discards `cmQuit` (modal stays open) and Alt-N can't switch
> background windows. We KEEP this (defensible UX; no menu/Alt-N trigger exists
> at row 34). **Phase-4 breadcrumb:** when menus + multiple windows + a modal
> coexist, suppress program-level command interception while a modal is active
> (C++'s nested `p->execute()` does this structurally).
>
> **`endModal` is downward (no up-pointer, D3).** `TDialog::handleEvent` cannot call
> `Program::endModal` (it has `&mut Context`, not the program). It signals through a
> new **`Deferred::EndModal(Command)`** variant (`ctx.end_modal(cmd)`); the pump
> applies it by setting `Program::end_state`, which the nested `exec_view` loop then
> observes. This is the unified-`Deferred`-channel rule from `69897fe` — *a new
> deferred capability adds a variant, not a `Context::new` param.* The pop lives in
> `exec_view` (not the `ModalFrame` handler): the handler holds no `&mut Program`,
> so it cannot reach the capture stack / `end_state` to decide a `valid(end_state)`
> conditional pop — only the owner-side `exec_view` can. `CaptureStack` gains a
> `pop()` for this (the one place a frame is removed other than `ConsumedPop`).

> **Capture-handler state must re-sync from the live tree (the cost of flattening
> the loops; found fixing a modal-dialog freeze).** Flattening C++'s *nested* loops
> into *sibling* capture handlers converts a structural fact into cached state — and
> cached state can desync. In C++, modality and drag are nested loops: `execView`
> dispatches through the **dialog's** `handleEvent` (`tgroup.cpp:205`), so the
> dialog *is* the modal dispatch root regardless of where it sits, and `dragView` is
> a further nested loop inside it. Position is never cached anywhere, so a dragged
> dialog cannot desync anything. Our `ModalFrame` instead gates positional events by
> the modal view's **bounds** — and it cached them at push time, so after a drag
> moved the dialog every click outside the *stale* rect was swallowed (with a lost
> `MouseUp` the keyboard goes too → total freeze). **Fix:** the loop calls
> `CaptureStack::sync_gate_bounds` (resolve each handler's `view()` id → live
> `get_bounds()` → `CaptureHandler::set_gate_bounds`, a **default no-op**) *before
> every dispatch*, so a bounds-gating handler follows its view through any
> move/resize path (incl. the resize check's direct `change_bounds`). The setter is
> a no-op by default so `DragCapture` — which *intentionally* snapshots its grab
> anchor for the drag's duration, exactly as C++ `dragView` does — is untouched.
> **General rule:** any capture handler that gates by live view-derived state must
> resync it from the tree each dispatch; C++'s nested loops get this for free, we
> pay for it explicitly. (Code: `src/capture.rs`, `src/app/program.rs`.)

> **`Clock` + `TimerQueue` shape (ratified during the row 20 port).** The
> injected clock is a `trait Clock { fn now_ms(&self) -> u64 }` (faithful to TV's
> `TTimePoint` = `uint64_t` ms tick), with a production `SystemClock` (the **only**
> place `Instant::now()` is allowed) and a test `ManualClock` (interior-mutable
> `Cell<u64>`, `set`/`advance`) — the latter is what makes the synchronous
> `pump_until_idle` loop deterministic. `TTimerQueue` ports faithfully *except*
> two structural deviations: **(1)** the `collectId` re-entrancy marking is
> **dropped** — `collect_expired(now_ms) -> Vec<TimerId>` gathers the due ids and
> the caller dispatches afterward, so there is no re-entrant list mutation to
> guard against (invariants preserved: one `now_ms` per pass; a periodic timer
> fires at most once per call, rescheduling forward via the **verbatim** port of
> `calcNextExpiresAt` — a catch-up-aware grid alignment, *not* `expires_at +=
> period`). **(2)** the clock is **not stored** in the queue — `now_ms` is passed
> into `set_timer`/`collect_expired`/`time_until_next` from the loop. `TTimerId`
> (a raw `TTimer*`) becomes an opaque monotonic `TimerId(u64)` — no generational
> reuse needed (unlike `ViewId`, D3), since a `u64` id space never realistically
> exhausts.

---

## D10 — Flat-record data transfer → typed value protocol · *forced*

**Baseline.** Each view reports `dataSize()` and marshals itself via `memcpy`
into/out of a caller-supplied packed C struct at hand-computed byte offsets; a
group walks its children summing sizes (`getData`/`setData`).

**Deviation.** Untyped `memcpy` of arbitrary bytes into a struct layout is
undefined-behavior territory in Rust. Each control instead exposes a typed
`value()` / `set_value()` over a `FieldValue` (or serde for an app struct); the
dialog gathers/scatters an ordered set of typed values. The validator's
`transfer` hook (D2) produces the typed value.

**Integration.** The gather/scatter *order* and the dialog↔controls flow port
faithfully; only the wire format changes from raw bytes to typed values.

---

## D11 — Platform layer → Backend trait (+ headless testing) · *chosen*

**Baseline.** `THardwareInfo` plus the termio / ansi / ncurses / win32 strategies
abstract the terminal, selected at runtime. There is no test harness.

**Deviation.** A small **`Backend` trait** (report size; flush changed cells;
cursor; an associated `EventSource`; clipboard as the landed `TClipboard`-order
**fallback chain** — copy: OS-native via arboard (`os-clipboard` feature, on by
default) → OSC 52 emit → internal buffer; paste: native → internal, no OSC 52
read — see `docs/design/os-clipboard.md`). Two impls:

- **`CrosstermBackend`** *(production)* — wraps crossterm; crossterm is a
  dependency hidden *behind* the trait.
- **`HeadlessBackend`** *(tests)* — in-memory cell buffer + programmable event
  queue; no TTY.

The rendering core is a **vendored** copy of ratatui's `Buffer`/`Cell` + cell
diff (the engine D8 relies on), adapted for TV roles.

This seam unlocks snapshot testing, which the C++ never had: a
**synchronously-pumpable loop** (`app.pump_until_idle()` — "idle" = empty queue +
no timer due, no sleeps/heuristics), the **injected clock** (D9), and golden
snapshots stored via `insta` as a text layer + style layer + legend + cursor
metadata (no timestamps). Each test owns its own `HeadlessBackend`, so the suite
runs in parallel.

**Integration.** The platform-selection *idea* ports faithfully (a swappable
adapter); the trait is just its Rust shape, with everything above the seam
identical in both modes. Testing is a pure addition.

> **`Backend` is object-safe; the `EventSource` collapses into `poll_event`
> (amendment, ratified during the row 18/19 design).** The trait above describes
> an *associated `EventSource`*; we instead make `Backend` **object-safe** so the
> app holds a `Box<dyn Backend>` and the view tree / `Context` never carry a
> `<B>` type parameter (the whole tree is already `Box<dyn View>` — a viral
> `<B: Backend>` would fight that). Concretely: `draw(&mut self, content:
> &[(u16, u16, &Cell)])` takes the **diff as a slice** (collected once per frame —
> trivial cost) rather than ratatui's generic `draw<I: Iterator>`; events come
> from `poll_event(&mut self, timeout: Option<Duration>) -> Option<Event>` rather
> than an associated type. The **`Renderer`** owns the back/front `Buffer` pair
> and runs paint → `Buffer::diff` → `backend.draw` → swap → `backend.flush` (the
> role of ratatui's `Terminal::draw`). `CrosstermBackend` translates crossterm
> events → our `Event` (the D4 key decomposition) and applies the row-5
> quantization ladder per the terminal's color depth; `HeadlessBackend` holds an
> in-memory front `Buffer` + a programmed event queue and **never blocks on the
> `poll_event` timeout** (it returns the next queued event or `None` immediately;
> time advances only via `ManualClock`, D9) — that is what makes
> `pump_until_idle` deterministic.

> **Golden snapshot format (FOUNDATION — frozen in `screen::snapshot`).** Every
> widget test from Phase 1 on diffs its `HeadlessBackend` screen against the
> string produced by `screen::snapshot::snapshot(&Buffer, cursor)`. The shape is
> fixed *once*, here, not improvised per test (re-baselining many goldens later
> is miserable). It has four parts, all timestamp-free: a **`size:`** line; a
> **`cursor:`** line (`x,y` or `hidden`); a **`text:`** layer (the glyphs, one
> `|`-framed line per row so trailing spaces survive); an **`attr:`** layer (one
> legend key per *display column*, aligned under the text — `.` is always the
> default style, others keyed `a..z A..Z 0..9` in row-major first-appearance
> order); and a **`legend:`** mapping each key to `fg=… bg=… [+mod…]` (the
> default style shown as the shorthand `default`). A wide glyph contributes its
> 2-column glyph to `text` and its key **twice** to `attr`; the `trail` cell is
> absorbed, so both layers are exactly `width` columns and stay aligned.

---

## D12 — TStreamable persistence → dropped (serde if revived) · *chosen*

**Baseline.** A hand-rolled reflection + type-registry + factory + serialization
(`ipstream`/`opstream`, `TStreamableClass`, `__DELTA`), plus resource files
(`TResourceFile`, `.res`) built on it. All of it exists because 1991 C++ had no
reflection or serialization.

**Deviation.** Do **not** port the machinery — Rust has `serde` (+ `typetag` for
trait objects). Defer the capability; if persistence is ever wanted, reach for
serde on the specific data worth saving.

**Integration.** Nothing in the core depends on it. Resource-file loading, if
needed later, becomes serde config or embedded assets — scoped to the real need.

---

## D13 — byte/char text → Unicode grapheme model · *minor*

**Baseline.** magiblot's `TText` already does width-aware Unicode (width-2 glyphs
+ continuation cells), but iterates per-`char`.

**Deviation.** Use `unicode-width` for columns and `unicode-segmentation` for
graphemes; **cluster combining marks / ZWJ sequences into a single cell** (the
delta beyond per-`char`). Mirror `TText`'s primitives: `width(&str)`,
`scroll(text, cols, include_incomplete) -> (byte_len, width)` (for a column
boundary that splits a double-width glyph), and a cell-writer taking a per-cell
`transform_attr` closure (so one blit can also apply selection styling).

**Integration.** All layout/cursor math measures in **display columns**, exactly
as `TText` does; only the clustering is stricter. A width-2 cell is followed by a
blank continuation cell.

> **D13 sub-decisions ratified during the row 8 port:**
> - *Unprintable chars → `�`, not CP437.* magiblot routes control bytes and
>   extended-ASCII through a CP437 translation table (the classic "☺ shows up in
>   the text" behaviour a TV veteran expects). The UTF-8-native port **drops the
>   CP437 table**: a control char (one with no `unicode-width`) is drawn as the
>   replacement glyph U+FFFD `�`, width 1. *(chosen — a visible behaviour change)*
> - *`measure` counts grapheme clusters.* `TTextMetrics::characterCount` counts
>   codepoints; our `TextMetrics::character_count` counts **grapheme clusters**
>   (equal except across combining sequences — C++ counts `e`+◌́ as 2, we count 1).
>   `grapheme_count` is the number of clusters with width > 0. If a future consumer
>   genuinely needs the codepoint count, add it then.

---

## D14 — DOS drives/backslash paths → native Linux `/` filesystem · *moderate*

**Baseline.** The file-dialog cluster (`TDirListBox`, `TFileList`, `TFileDialog`,
`TChDirDialog`; rows 75–80) is DOS-flavored: backslash (`\`) separators, A:–Z:
drive letters, a literal **"Drives"** tree entry, `getdisk`/`driveValid`/
`getcurdir`. magiblot keeps `\`-style strings inside the TV layer and translates
to `/` at the syscall boundary (`path_dos2unix` in `source/platform/findfrst.cpp`),
emulating a *single* disk on UNIX; it already `#if !defined(_TV_UNIX)`-drops the
"Drives" entry on UNIX.

**Deviation.** rstv is a **native-Linux** port — model paths as Linux paths
end-to-end, no DOS layer to translate:
- **Separator is `/`**, never `\`. The tree root is **`/`** (not `C:\`).
- **No drives.** Drop `showDrives`, the `drives`/`"Drives"` entry, the A–Z scan,
  `getdisk`/`driveValid`/`getcurdir`. `TDirListBox::newDirectory` has only the
  `showDirs` branch.
- **`showDirs` walks `/`-segments:** root `/` at the top, each ancestor directory
  indented one extra level, then the current directory's immediate
  subdirectories. Enumerate subdirs with **`std::fs::read_dir`** filtering to
  directories, skipping names starting with `.`, sorted (case-insensitive, the
  row-70 ordering). The DOS `findfirst(FA_DIREC)` loop → a `read_dir` filter.
- **Tree glyphs** keep the C++ structure (`pathDir`/`firstDir`/`middleDir`/
  `lastDir` connector strings + the last-entry `graphics` fix-up) but render via
  D7 Theme glyphs (CP437 box-drawing → the project's Unicode box-drawing).

**Integration.** Inherited by **all of rows 75–80** — they share one `/`-native
path model; there is no `\`↔`/` translation seam anywhere. `pathValid`/`isDir`/
`validFileName` (in `TFileList`/dialog rows) likewise become thin `std::fs`/
`std::path` wrappers: root `/` is always valid; a path is valid if it `is_dir()`.

---

## D15 — DOS `findfirst` local time → `std::fs` mtime, UTC · *minor*

**Baseline.** `TFileList::readDirectory` reads each entry's timestamp from the DOS
`findfirst`/`ffblk` as a packed `ftime` 32-bit word (`(ff_fdate<<16)|ff_ftime` —
year-1980/month/day in the high half, hour/min/sec÷2 in the low half, **local
time**), stored on `TSearchRec::time`. `TFileInfoPane::draw` unpacks that bitfield
to render `Mon DD, YYYY HH:MMa/p` (row 78).

**Deviation.** rstv reads the timestamp from **`std::fs::Metadata::modified()`**
(a `SystemTime`) and packs it into the **same DOS `ftime` u32** so the info-pane
unpack ports **verbatim**. The civil date is computed **in UTC** (Howard Hinnant's
days-from-civil — no `chrono`/`time` crate dependency for one info pane). Edge
handling: pre-1980 (and `duration_since` errors) clamp to the DOS epoch
(`0x0021_0000` = Jan 01 1980 00:00); far-future years (≥2044) intentionally set
the `i32` sign bit and round-trip through `as u32` at unpack; the synthesized `..`
entry uses the epoch constant unconditionally (C++ stats the real parent — a
cosmetic date difference on that one row).

**Integration.** Confined to `FileList::build_listing` (the pack) and
`FileInfoPane::draw` (the verbatim unpack), both in `src/dialog/filedlg.rs`. The
**UTC vs local** display difference is the only user-visible divergence; accepted
to avoid a timezone dependency. `TSearchRec::time` keeps the faithful DOS layout
so the C++ draw code is unchanged.

---

## Vendoring & licensing

- **ratatui** cell-buffer + diff is **copied** (not depended on) and adapted —
  keep its MIT header in the vendored file(s) and note it in the project NOTICE.
- **crossterm** (MIT) is a normal dependency, behind `Backend` (D11).
- The port carries forward Borland's public-domain Turbo Vision and magiblot's
  MIT terms in the NOTICE.

---

## rstv-original extensions (beyond the faithful port)

These features have **no C++ tvision counterpart** — they are rstv inventions
added alongside the faithful port. They do NOT belong in the C++→Rust symbol
lookup (Appendix A) or the per-class recipe (Appendix B).

### `RegexValidator` (`src/widgets/input_line.rs`)
A regex-driven string validator using the `regex` crate. The *faithful* port of
the C++ picture-mask DSL is `PXPictureValidator`; `RegexValidator` is the modern
alternative living alongside it. Both implement the `Validator` trait. Use
`RegexValidator` when the picture-mask DSL is too rigid.

### Truecolor color-picker (`src/dialog/colorpick/`, `Program::color_dialog`)
Replaces the dropped `TColorDialog` cluster (rows 81–87). The faithful cluster
edited a flat BIOS `TPalette` that rstv deletes under D7 (palette → `Theme`;
`Role` is a closed enum) — a faithful port would produce dead code by
construction. The truecolor picker is reusable and produces any `Color` variant:

- **`ColorPicker`** (`dialog::ColorPicker`) — the reusable embeddable view.
  Owns a shared `ColorModel` + four tabbed surfaces (Presets, RGB+hex, HSV plane,
  xterm-256 grid). `color() -> Color` is the result contract.
- **`Program::color_dialog(initial: Color) -> Option<Color>`** — the modal entry
  point. Returns `Some(color)` on OK, `None` on Cancel/Esc.

See [`docs/superpowers/specs/2026-06-09-color-picker-design.md`](file:///home/oetiker/checkouts/rstv/docs/superpowers/specs/2026-06-09-color-picker-design.md)
and [`docs/superpowers/plans/2026-06-09-color-picker.md`](file:///home/oetiker/checkouts/rstv/docs/superpowers/plans/2026-06-09-color-picker.md).

---

## Appendix A — C++ → Rust deviation lookup

> The cheat-sheet for translating any C++ symbol touched by a deviation.
> Anything not here ports verbatim. Filled in as subsystems land.

| C++ | Rust | deviation |
| --- | ---- | --------- |
| `TView` | `tv::View` (trait) + `tv::ViewState` (data) | D2 |
| `TGroup` | `tv::Group` (owns `Vec<Box<dyn View>>`) | D2, D3 |
| `TWindow`, `TDialog`, `TButton`, … | `tv::Window`, `tv::Dialog`, `tv::Button`, … | D1 |
| `cmOK` / `cmCancel` | `tv::Command::OK` / `::CANCEL` — `Command(&'static str)`, open newtype, namespaced | D1 |
| `kbEnter` | `tv::Key::Enter` (enum) + `KeyModifiers` (decomposed) | D4, D1 |
| `TKey` / `KeyDownEvent` | `event::{Key, KeyModifiers, KeyEvent}` | D1, D4, D5 |
| `sfFocused` | `state.focused` / `StateFlag::Focused` | D5 |
| `ofSelectable` / `ofPreProcess` | `options.selectable` / `options.pre_process` | D5 |
| `growMode` / `dragMode` | `GrowMode` / `DragMode` (struct-of-bools; `gf*`/`dm*`) | D5 |
| `helpCtx` / `hcNoContext` | `ViewState.help_ctx` / `HelpCtx::NO_CONTEXT` (open newtype) | D1 |
| `evKeyDown` / `evCommand` | `Event::KeyDown(..)` / `Event::Command(..)` | D4 |
| `message(v, …)` | targeted query (`Option<T>`) / `Event::Broadcast` | D4 |
| `getPalette` / `getColor` | `ctx.theme.style(Role::…)` | D7 |
| hardcoded glyph tables (`frameChars`, `TScrollChars`, …) | fields on `theme::Glyphs`, read via `ctx.glyphs()` (grows per-widget; row-9 convention) | D7 |
| `TColorAttr` / `TColorDesired` | `Style` / `Color` (4-variant enum) | D6 |
| `execView` | `exec_view` → result via posted `Command` | D9 |
| `dragView` / press-tracking | capture-stack handlers | D9 |
| `getData` / `setData` / `dataSize` | typed `value` / `set_value` protocol | D10 |
| `TValidator` family | `Validator` trait + impls | D2 |
| `THardwareInfo` / `TScreen` | `Backend` trait (`Crossterm`/`Headless`) | D11 |
| `TText` | `text` module (`width`/`scroll`/cell-writer) | D13 |
| `TDrawBuffer` | `screen::DrawBuffer` (`move_char`/`move_str`/`move_cstr`/`move_buf`) | D6, D8, D13 |
| `owner` / `current` / `selected` | `ViewId` handles | D3 |
| `drawHide`/`drawShow`/`drawUnder*`, buffered group | — (dropped; redraw + diff) | D8 |
| `TStreamable`, `TResourceFile` | — (dropped; serde if revived) | D12 |
| `TCommandSet` (256-bit) | `CommandSet` over `HashSet<Command>` (no range guard; `Program` stores `curCommandSet` as a **disabled set**, denylist) | D1 |
| `forEach` / `firstThat` / `TSortedCollection` | iterators / `Vec<T: Ord>` | (idiom) |

---

## Appendix B — Per-class porting procedure

> The mechanical recipe for any class tagged **`MECHANICAL`** in
> [PORT-ORDER.md](file:///home/oetiker/checkouts/rstv/docs/PORT-ORDER.md).
> Follow it verbatim. **Do not run this recipe on `FOUNDATION` or `INFRA` rows**
> — those establish or build the patterns and need careful (human/Opus) work.
> See the **Escalate** list at the end for when to stop and ask.

**0 · Preconditions.** Confirm every row this class depends on (its base class
and everything it owns/constructs) is already ported. Open the C++ file(s) named
in the row's `C++ files` column under
`/home/oetiker/scratch/tvision-spec/magiblot-tvision/source/tvision/` (plus the
matching header in `include/tvision/`).

**1 · Module & type (D1, D2, D3, D5, D6).** Create the Rust module from the
row's `Rust module` column. Define the struct, dropping the `T` prefix and
embedding `ViewState`:

```rust
pub struct Foo {                 // TFoo -> Foo
    base: ViewState,             // the TView data members
    // ...the class's own C++ data members, translated:
}
```
Translate members: `TView*`/`owner`/peer pointers → `ViewId`; the `ushort`
`state`/`options`/`growMode`/`dragMode` are already in `base`; color fields →
`Style`; C strings → `String`; `TCollection*` → `Vec<_>`.

**2 · Trait impl (D2).** `impl tv::View for Foo { … }`, providing `state` /
`state_mut` / `draw`, and overriding `handle_event` and any other virtuals the
C++ class overrides. If the C++ class derives a *concrete* widget (e.g.
`TParamText : TStaticText`), **embed-and-delegate**: hold the base widget and
forward the unchanged methods.

**3 · Port each method body verbatim, applying these line-level substitutions:**

| C++ pattern | Rust | dev |
| ----------- | ---- | --- |
| `TFoo`, `new TFoo(...)`, `meth()` | `Foo`, `Foo::new(...)`, `meth()` snake_case | D1 |
| `cmX` / `hcX` | `tv::Command::X` / `tv::HelpCtx::X` (open-newtype assoc consts) | D1 |
| `kbX` | PascalCase `enum Key` variant (`kbEnter`→`Key::Enter`); combined codes decompose into base `Key` + `KeyModifiers` (`kbCtrlC`→`Key::Char('c')`+ctrl, `kbShiftTab`→`Key::Tab`+shift) | D4/D5 |
| `event.what == evX`, `& evX` | `match ev { Event::X(..) => … }` | D4 |
| `clearEvent(event)` | `*ev = Event::Nothing` | D4 |
| `state & sfX` / `setState(sfX,v)` | `self.state().x` / `self.set_state(StateFlag::X, v)` | D5 |
| `options & ofX` | `self.state().options.x` | D5 |
| `getColor(n)` / `getPalette()` | `ctx.theme.style(Role::…)` | D7 |
| literal box/mark chars | `ctx.theme.glyphs.…` | D7 |
| `TDrawBuffer` + `writeLine`/`writeBuf` | write cells into `ctx` at local coords | D8 |
| string width / truncation | `text::width` / `text::scroll` | D13 |
| `owner` / `current` / `TopView()` | resolve via `ctx` / `ViewId` (never store up-ptr) | D3 |
| `message(rcvr, evBroadcast, cmX, p)` | `ctx.broadcast(Command::X)` | D4 |
| `message(rcvr, …)` expecting a result | `ctx.query(id, …) -> Option<T>` | D4 |
| `dataSize` / `getData` / `setData` | typed `value()` / `set_value()` | D10 |
| `streamableName`/`read`/`write`/`build`, `TStreamableClass` reg | **delete** | D12 |

**4 · Snapshot test (mandatory, D11).** Build the widget on a `HeadlessBackend`,
`pump_until_idle()`, and `assert_snapshot!(app.backend().buffer())`. For
interactive widgets, push key/mouse events and snapshot each step; advance the
injected `Clock` for any animation.

**5 · Verify.** The widget must respond to the same keys/commands and lay out the
same as the C++ original. The snapshot is the evidence.

**Escalate — stop and hand back to a human/Opus — when:**
- the row is tagged `FOUNDATION` or `INFRA`;
- a method does pointer arithmetic, `union` field access, or relies on dropped
  machinery (`lock`/`unlock`/`buffer`, `drawUnder*`, streaming);
- the `s*`/`nm*` file holds **real member code**, not just streamable boilerplate
  (PORT-ORDER.md flags these explicitly);
- a `getData`/`setData` does a raw struct `memcpy` whose record layout isn't
  obvious (the typed `value` mapping is a judgment call — D10);
- behavior depends on a class not yet ported.
