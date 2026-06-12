# Deviations D1–D13

`tvision` is a *faithful* port of magiblot's C++ Turbo Vision: class structure,
method names, control flow, and behaviour are reproduced as-is — **except** in
the handful of places where a line-by-line port is impossible or unwise in Rust.
Each such place is a numbered **deviation**. If a behaviour is *not* on this
list, it was ported straight from the C++.

Each deviation carries a tag. Most are **forced** (Rust literally won't compile
the direct port) or **chosen** (the direct port compiles, but a modern construct
is clearly better); a few of the later ones are tagged by scope instead
(**minor** / **moderate**) where the change is small and self-contained. The
full write-up follows a fixed shape — *Baseline* (what the C++ does) →
*Deviation* (what we do instead) → *Integration* (how the rest of the faithful
port plugs back in).

This page is a one-line index. The authoritative text — including the
*Integration* notes and every later ratification — lives in
[`docs/PORTING-GUIDE.md`](https://github.com/oetiker/rstv/blob/main/docs/PORTING-GUIDE.md).
For the *narrative* behind the ones that matter most to a Turbo Vision veteran,
read **[The Idiomatic Port](../port/faithful.md)** (Part II), linked per row
below.

| # | Deviation | Tag | Narrative |
| --- | --------- | --- | --------- |
| **D1** | Names & namespacing: drop the `T` prefix, `tv::` house style, `snake_case` methods; `cm*`/`hc*` constant families → type-scoped associated consts on open newtypes (`Command`, `HelpCtx`). | chosen | [Constant families](../port/constants.md) |
| **D2** | Inheritance → a `View` **trait** with default methods plus a `ViewState` struct held by **composition**; "subclass `TWindow`" becomes embed-and-delegate. | forced | [Inheritance](../port/inheritance.md) |
| **D3** | Raw `TView*` pointers → process-global `ViewId` handles + a downward-passed `Context`; up/sideways links resolve by tree-walk, never by reference. | forced | [Pointers & infoPtr → handles](../port/handles.md) |
| **D4** | `TEvent` tagged union + bitmasks → `enum Event` matched not masked; `message()` splits into a targeted query and an `Event::Broadcast`; `TKey`/`kb*` → a closed `enum Key` + `KeyModifiers`. | chosen | [Events](../port/events.md) |
| **D5** | Bit-word flags (`state`, `options`, `growMode`, …) → `#[derive(Default)]` structs of bools, with a verb-enum `set_state` over them. | chosen | [Flag words](../port/flags.md) |
| **D6** | Packed attribute bytes (`TColorAttr`/`TColorDesired`) → a typed four-variant `Color` enum + `Style`; the per-cell retain-`0` overloads are dropped. | chosen | [The draw model](../port/draw.md) |
| **D7** | Palette chains + scattered glyph literals → a `Theme` owning a state→`Role` style map and a `Glyphs` set; `getColor`/`getPalette` → `ctx.theme.style(Role::…)`. | chosen | [Palettes & glyphs → Theme](../port/theme.md) |
| **D8** | Per-write occlusion + damage tracking → whole-tree redraw into a back buffer + a diff-bounded terminal flush; occlusion becomes the painter's algorithm; shadows are cast during the draw. | chosen | [The draw model](../port/draw.md) |
| **D9** | Nested blocking modal loops (`execView`, `dragView`) → **one** non-recursive event loop plus a LIFO **capture stack**; modality, drag, and press-tracking are handlers, not loops. | forced | [Modal execView](../port/modal.md) |
| **D10** | Flat-record `memcpy` data transfer (`getData`/`setData`/`dataSize`) → a typed `value()`/`set_value()` protocol over a `FieldValue`. | forced | [Dialogs & data](../apps/dialogs.md) |
| **D11** | The platform layer (`THardwareInfo`, ncurses/win32 strategies) → a small object-safe `Backend` trait, with a production `CrosstermBackend` and a test `HeadlessBackend` that unlocks snapshot testing. | chosen | [Drawing & backends](../internals/drawing.md) |
| **D12** | `TStreamable` persistence + resource files → **dropped**; reach for `serde` if persistence is ever wanted. | chosen | [Dropped & changed](../port/dropped.md) |
| **D13** | Per-`char`, CP437 text → a Unicode grapheme model (`unicode-width` + `unicode-segmentation`), clustering combining marks into one cell; unprintables render as `�`. | minor | [Dropped & changed](../port/dropped.md) |

## Beyond D13

Two further deviations cover the file-dialog cluster, which the C++ models in
DOS terms:

- **D14** — DOS drives and `\` paths → a native Linux `/` filesystem (no drive
  letters, root is `/`, subdirectories via `std::fs::read_dir`). *moderate.*
- **D15** — DOS `findfirst` local-time stamps → `std::fs` mtime computed in UTC,
  packed into the same DOS `ftime` word so the info-pane unpack ports verbatim.
  *minor.*

The [`Deferred` channel](../port/deferred.md) and the [cross-view sibling
broker](../internals/brokering.md) are not numbered deviations but the
*substrate* that several of these (notably D3 and D9) rely on; see Part IV.
