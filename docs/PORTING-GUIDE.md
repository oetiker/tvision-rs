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
- Crate published as `tvision`; **house style `tv::`** on every type/constant
  (consumers add `tv = { package = "tvision" }` once, no `use` needed). The path
  *is* the namespace the `T` prefix was faking.
- Drop the `T` prefix (`TView` → `tv::View`). Methods are `snake_case`
  (`handleEvent` → `handle_event`).
- The `cm*`/`kb*`/`hc*` families become **type-scoped associated constants** in
  `SCREAMING_SNAKE_CASE` on open newtypes — recognizable, no `#![allow]`, and
  user-extensible (the value space stays open, unlike an `enum`):

  ```rust
  pub struct Command(pub u16);
  impl Command {
      pub const OK:     Command = Command(10);
      pub const CANCEL: Command = Command(11);
  }
  // tv::Command::OK ; your app: pub const CMD_REFRESH: tv::Command = tv::Command(1000);
  ```

  Likewise `tv::Key::F1`, `tv::HelpCtx::NO_CONTEXT`. (`sf*`/`of*` are handled by
  D5.) Because `Command` is an open `u16` rather than a 0–255 space, the
  enabled-command set is a `HashSet<Command>`, not a fixed bit array.

> **Key values are modern, not BIOS scan codes.** `kbEnter = 0x1c0d` is a DOS
> scan-code/ASCII pair. We keep the *name* (`tv::Key::ENTER`) but build the value
> from crossterm's key model. The name transfers; the magic number does not.

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
- **Up/sideways links are `ViewId` handles** (a generational arena index), never
  `&View`. Identity is `ViewId` equality.
- **No up-pointers:** the parent passes `&mut Context` / `DrawCtx` *down*,
  carrying everything a child would otherwise reach upward for — including the
  **resolved parent/background style and the current clip rect**.

**Integration.** Focus/selection/traversal *logic* ports faithfully (tab order,
the per-group `current` vs. global `focused` distinction, validate-on-focus-leave,
`makeFirst` raising a window). Only two substitutions: pointer → `ViewId`, and
"reach upward" → "read from `Context`." A targeted `message(view, …)` becomes a
`Context` query addressed by `ViewId` (see D4).

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

**Integration.** `exec_view` can't block-and-return its result; the result is
delivered when the modal handler pops — by **posting a completion `Command`** (or
invoking a callback) to the owner. Animation / cursor-blink / auto-repeat
schedule against an **injected `Clock`** with cancelable handles and set the
loop's poll timeout — never `sleep` (this is also what makes timing
deterministic under test, D11).

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
cursor; an associated `EventSource`; clipboard with internal-buffer fallback,
mirroring TV's negotiated OSC 52). Two impls:

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

---

## Vendoring & licensing

- **ratatui** cell-buffer + diff is **copied** (not depended on) and adapted —
  keep its MIT header in the vendored file(s) and note it in the project NOTICE.
- **crossterm** (MIT) is a normal dependency, behind `Backend` (D11).
- The port carries forward Borland's public-domain Turbo Vision and magiblot's
  MIT terms in the NOTICE.

---

## Appendix A — C++ → Rust deviation lookup

> The cheat-sheet for translating any C++ symbol touched by a deviation.
> Anything not here ports verbatim. Filled in as subsystems land.

| C++ | Rust | deviation |
| --- | ---- | --------- |
| `TView` | `tv::View` (trait) + `tv::ViewState` (data) | D2 |
| `TGroup` | `tv::Group` (owns `Vec<Box<dyn View>>`) | D2, D3 |
| `TWindow`, `TDialog`, `TButton`, … | `tv::Window`, `tv::Dialog`, `tv::Button`, … | D1 |
| `cmOK` / `cmCancel` | `tv::Command::OK` / `::CANCEL` | D1 |
| `kbEnter` | `tv::Key::ENTER` (modern value) | D1 |
| `sfFocused` | `state.focused` / `StateFlag::Focused` | D5 |
| `ofSelectable` / `ofPreProcess` | `options.selectable` / `options.pre_process` | D5 |
| `evKeyDown` / `evCommand` | `Event::KeyDown(..)` / `Event::Command(..)` | D4 |
| `message(v, …)` | targeted query (`Option<T>`) / `Event::Broadcast` | D4 |
| `getPalette` / `getColor` | `ctx.theme.style(Role::…)` | D7 |
| `TColorAttr` / `TColorDesired` | `Style` / `Color` (4-variant enum) | D6 |
| `execView` | `exec_view` → result via posted `Command` | D9 |
| `dragView` / press-tracking | capture-stack handlers | D9 |
| `getData` / `setData` / `dataSize` | typed `value` / `set_value` protocol | D10 |
| `TValidator` family | `Validator` trait + impls | D2 |
| `THardwareInfo` / `TScreen` | `Backend` trait (`Crossterm`/`Headless`) | D11 |
| `TText` | `text` module (`width`/`scroll`/cell-writer) | D13 |
| `owner` / `current` / `selected` | `ViewId` handles | D3 |
| `drawHide`/`drawShow`/`drawUnder*`, buffered group | — (dropped; redraw + diff) | D8 |
| `TStreamable`, `TResourceFile` | — (dropped; serde if revived) | D12 |
| `TCommandSet` (256-bit) | `HashSet<Command>` | D1 |
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
| `cmX` / `kbX` / `hcX` | `tv::Command::X` / `tv::Key::X` / `tv::HelpCtx::X` | D1 |
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
