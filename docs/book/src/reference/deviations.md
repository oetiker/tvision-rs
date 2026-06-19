# Differences from C++ Turbo Vision

tvision-rs is a *faithful* port of magiblot's C++ Turbo Vision: class structure,
method names, control flow, and behaviour are reproduced as-is — **except** in
the handful of places where a line-by-line port is impossible or unwise in Rust.
Each such place is a numbered difference. If a behaviour is *not* on this list,
it was ported straight from the C++.

Each difference carries a tag: most are **forced** (Rust literally won't compile
the direct port) or **chosen** (the direct port compiles, but a modern construct
is clearly better); a few are tagged by scope (**minor** / **moderate**) where
the change is small and self-contained.

This page is the at-a-glance list. Every entry links to the Part II chapter that
tells its story, and — where one type carries the difference — to that type's API
reference. Other pages cite an entry by its stable anchor (for example
[difference D2](#d2)).

Porting contributors: see the project repository.

### D1 · Names & namespacing {#d1}

*chosen.* The `T` prefix is dropped and every type lives under the `tv::`
namespace (`TButton` → `tv::Button`); methods become `snake_case`. The
`cm*`/`hc*` constant families become type-scoped associated constants on open
newtypes ([`Command`](../api/tvision-rs/command/struct.Command.html),
[`HelpCtx`](../api/tvision-rs/help/struct.HelpCtx.html)) so apps can mint their own
values. → [Constant families](../port/constants.md)

### D2 · Inheritance → trait + composition {#d2}

*forced.* The class tree becomes a
[`View`](../api/tvision-rs/view/trait.View.html) trait with default methods plus a
[`ViewState`](../api/tvision-rs/view/struct.ViewState.html) struct held by
composition; "subclass `TWindow`" becomes embed-and-delegate — hold a
[`Window`](../api/tvision-rs/window/struct.Window.html), forward the methods you
don't change. → [Inheritance](../port/inheritance.md)

### D3 · Pointers → handles + downward context {#d3}

*forced.* Raw `TView*` pointers become process-global
[`ViewId`](../api/tvision-rs/view/struct.ViewId.html) handles plus a downward-passed
[`Context`](../api/tvision-rs/view/struct.Context.html); up/sideways links resolve
by tree-walk, never by reference. → [Pointers & infoPtr → handles](../port/handles.md)

### D4 · Events → enum + match {#d4}

*chosen.* `TEvent`'s tagged union and bitmasks become an
[`Event`](../api/tvision-rs/event/enum.Event.html) enum that is matched, not masked;
`message()` splits into a targeted query and an `Event::Broadcast`. `TKey`/`kb*`
become a closed [`Key`](../api/tvision-rs/event/enum.Key.html) enum plus
[`KeyModifiers`](../api/tvision-rs/event/struct.KeyModifiers.html). → [Events](../port/events.md)

### D5 · Flag words → struct-of-bools {#d5}

*chosen.* The `ushort` flag words (`state`, `options`, `growMode`, `dragMode`)
become `#[derive(Default)]` structs of bools, with a verb-enum `set_state` over
them ([`StateFlag`](../api/tvision-rs/view/enum.StateFlag.html),
[`Options`](../api/tvision-rs/view/struct.Options.html)). → [Flag words](../port/flags.md)

### D6 · Attribute bytes → typed Color/Style {#d6}

*chosen.* Packed `TColorAttr`/`TColorDesired` bytes become a typed four-variant
[`Color`](../api/tvision-rs/color/enum.Color.html) enum plus
[`Style`](../api/tvision-rs/color/struct.Style.html); the per-cell retain-`0`
overloads are dropped. → [The draw model](../port/draw.md)

### D7 · Palettes & glyphs → Theme {#d7}

*chosen.* Palette chains and scattered glyph literals become a
[`Theme`](../api/tvision-rs/theme/struct.Theme.html) owning a state→`Role` style map
and a `Glyphs` set; `getColor`/`getPalette` become
`ctx.theme.style(`[`Role`](../api/tvision-rs/theme/enum.Role.html)`::…)`. → [Palettes & glyphs → Theme](../port/theme.md)

### D8 · Whole-tree redraw + diff {#d8}

*chosen.* Per-write occlusion and damage tracking become a whole-tree redraw into
a back buffer plus a diff-bounded terminal flush; occlusion becomes the painter's
algorithm, and shadows are cast during the draw. → [The draw model](../port/draw.md)

### D9 · Modal loops → one loop + capture stack {#d9}

*forced.* Nested blocking modal loops (`execView`, `dragView`) become **one**
non-recursive event loop plus a LIFO capture stack; modality, drag, and
press-tracking are handlers, not loops. `execView` from a `Program` method
becomes [`Program::exec_view_with`](../api/tvision-rs/app/struct.Program.html#method.exec_view_with)
(result by value); `execView` from within a view becomes `Context::request_exec_view`
(queues `Deferred::OpenModal`; reuses the existing `pending_modal` +
`RouteModalAnswer` machinery; close command routed back via `set_modal_answer`).
→ [Modal execView](../port/modal.md)

### D10 · Data transfer → typed value protocol {#d10}

*forced.* Flat-record `memcpy` data transfer (`getData`/`setData`/`dataSize`)
becomes a typed `value()`/`set_value()` protocol over a
[`FieldValue`](../api/tvision-rs/data/enum.FieldValue.html). → [Dialogs & data](../apps/dialogs.md)

### D11 · Platform layer → Backend trait {#d11}

*chosen.* The platform layer (`THardwareInfo`, ncurses/win32 strategies) becomes
a small object-safe [`Backend`](../api/tvision-rs/backend/trait.Backend.html) trait,
with a production
[`CrosstermBackend`](../api/tvision-rs/backend/struct.CrosstermBackend.html) and a
test [`HeadlessBackend`](../api/tvision-rs/backend/struct.HeadlessBackend.html) that
unlocks snapshot testing. → [Drawing & backends](../internals/drawing.md)

### D12 · Persistence → dropped {#d12}

*chosen.* `TStreamable` persistence and resource files are dropped; reach for
`serde` if persistence is ever wanted. → [Dropped & changed](../port/dropped.md)

### D13 · Text → Unicode grapheme model {#d13}

*minor.* Per-`char`, CP437 text becomes a Unicode grapheme model
(`unicode-width` + `unicode-segmentation`) that clusters combining marks into one
cell; unprintables render as `�`. → [Dropped & changed](../port/dropped.md)

### D14 · DOS drives & paths → native Linux filesystem {#d14}

*moderate.* The file dialog's DOS drives and `\` paths become a native Linux `/`
filesystem: no drive letters, the root is `/`, and subdirectories are listed via
`std::fs::read_dir`. → [Dropped & changed](../port/dropped.md)

### D15 · DOS timestamps → `std::fs` mtime, UTC {#d15}

*minor.* DOS `findfirst` local-time stamps become `std::fs` mtime computed in
UTC, packed into the same DOS `ftime` word so the file-info-pane unpack ports
verbatim. → [Dropped & changed](../port/dropped.md)

## Underlying mechanisms

The [`Deferred` channel](../port/deferred.md) and the [cross-view sibling
broker](../internals/brokering.md) are not numbered differences but the
*substrate* that several of these (notably [D3](#d3) and [D9](#d9)) rely on; see
Part IV.
