# C++ Turbo Vision → `tv::` symbol map

A terse lookup for translating a C++ Turbo Vision symbol into its tvision-rs
equivalent (the `tvision-rs` crate, imported as `tv`). This is the *what*; the *why*
lives in [The Idiomatic Port](../port/faithful.md) and the summary in
[Differences from C++ Turbo Vision](deviations.md).

Two mechanical rules cover most of the table:

- **Drop the `T` prefix and namespace under `tv::`** — `TButton` → `tv::Button`.
- **`cmFoo` / `hcFoo` constant families** become associated consts on an open
  newtype — `cmOK` → [`tv::Command::OK`](../api/tvision_rs/command/struct.Command.html),
  `hcNoContext` → [`tv::HelpCtx::NO_CONTEXT`](../api/tvision_rs/help/struct.HelpCtx.html).

## Core types

| C++ Turbo Vision | tvision-rs (Rust) | notes |
| ---------------- | -------------- | ----- |
| `TView` | [`View`](../api/tvision_rs/view/trait.View.html) (trait) + [`ViewState`](../api/tvision_rs/view/struct.ViewState.html) (data) | behaviour is a trait; data is composed in, not inherited |
| `TGroup` | [`Group`](../api/tvision_rs/view/struct.Group.html) | owns `Vec<Box<dyn View>>` |
| `TFrame` | [`Frame`](../api/tvision_rs/frame/struct.Frame.html) | |
| `TWindow` | [`Window`](../api/tvision_rs/window/struct.Window.html) | |
| `TDialog` | [`Dialog`](../api/tvision_rs/dialog/struct.Dialog.html) | |
| `TDeskTop` | [`Desktop`](../api/tvision_rs/desktop/struct.Desktop.html) | |
| `TProgram` | [`Program`](../api/tvision_rs/app/struct.Program.html) | the engine: tree + event loop + backend |
| `TApplication` | [`Application`](../api/tvision_rs/app/struct.Application.html) | thin wrapper over `Program` |

See [The application skeleton](../getting-started/skeleton.md) for the
`Program` / `Application` split.

## Controls & widgets

| C++ Turbo Vision | tvision-rs (Rust) |
| ---------------- | -------------- |
| `TButton` | [`Button`](../api/tvision_rs/widgets/struct.Button.html) |
| `TStaticText` | [`StaticText`](../api/tvision_rs/widgets/struct.StaticText.html) |
| `TParamText` | [`ParamText`](../api/tvision_rs/widgets/struct.ParamText.html) |
| `TLabel` | [`Label`](../api/tvision_rs/widgets/struct.Label.html) |
| `TInputLine` | [`InputLine`](../api/tvision_rs/widgets/struct.InputLine.html) |
| `TCluster` | [`Cluster`](../api/tvision_rs/widgets/struct.Cluster.html) |
| `TCheckBoxes` | [`CheckBoxes`](../api/tvision_rs/widgets/struct.CheckBoxes.html) |
| `TRadioButtons` | [`RadioButtons`](../api/tvision_rs/widgets/struct.RadioButtons.html) |
| `TScrollBar` | [`ScrollBar`](../api/tvision_rs/widgets/struct.ScrollBar.html) |
| `TScroller` | [`Scroller`](../api/tvision_rs/widgets/struct.Scroller.html) |
| `TListViewer` | [`ListViewer`](../api/tvision_rs/widgets/list_viewer/trait.ListViewer.html) (trait) |
| `TListBox` | [`ListBox`](../api/tvision_rs/widgets/struct.ListBox.html) |
| `TOutline` | [`Outline`](../api/tvision_rs/widgets/outline/struct.Outline.html) / [`OutlineViewer`](../api/tvision_rs/widgets/outline/trait.OutlineViewer.html) |
| `TEditor` | [`Editor`](../api/tvision_rs/widgets/struct.Editor.html) |
| `TEditWindow` | [`EditWindow`](../api/tvision_rs/widgets/struct.EditWindow.html) |
| `TMenuBar` | [`MenuBar`](../api/tvision_rs/menu/menu_bar/struct.MenuBar.html) |
| `TMenu` / `TMenuItem` | [`Menu`](../api/tvision_rs/menu/struct.Menu.html) / [`MenuItem`](../api/tvision_rs/menu/enum.MenuItem.html) |
| `TStatusLine` / `TStatusItem` | [`StatusLine`](../api/tvision_rs/status/status_line/struct.StatusLine.html) / [`StatusItem`](../api/tvision_rs/status/struct.StatusItem.html) |
| `TValidator` family | [`Validator`](../api/tvision_rs/validate/trait.Validator.html) trait + impls |

The full set is in [Controls](../apps/controls.md) and
[Menus, status line & help](../apps/menus.md).

## Events, keys & commands

| C++ Turbo Vision | tvision-rs (Rust) | notes |
| ---------------- | -------------- | ----- |
| `TEvent` / `event.what == evX` | [`Event`](../api/tvision_rs/event/enum.Event.html) enum, matched | `evKeyDown` → `Event::KeyDown(..)`, `evCommand` → `Event::Command(..)` |
| `KeyDownEvent` | [`KeyEvent`](../api/tvision_rs/event/struct.KeyEvent.html) | |
| `MouseEventType` | [`MouseEvent`](../api/tvision_rs/event/struct.MouseEvent.html) | |
| `kbEnter`, `kbF1`, … | [`Key`](../api/tvision_rs/event/enum.Key.html) enum (`Key::Enter`, `Key::F(1)`) | combined codes decompose |
| `kbCtrlC`, `kbShiftTab` | base `Key` + [`KeyModifiers`](../api/tvision_rs/event/struct.KeyModifiers.html) | `Key::Char('c')` + ctrl |
| `clearEvent(event)` | `*ev = Event::Nothing` | |
| `cmOK`, `cmCancel`, … | [`Command`](../api/tvision_rs/command/struct.Command.html) assoc consts | open newtype, namespaced |
| `TCommandSet` (256-bit) | [`CommandSet`](../api/tvision_rs/command/struct.CommandSet.html) | `Program` stores the *disabled* set (denylist) |
| `message(rcvr, evBroadcast, cmX, p)` | `ctx.broadcast(Command::X)` | |
| `message(...)` expecting a result | targeted query → `Option<T>` | |

How events route is covered in [Events → enum + match](../port/events.md) and
[Commands & events](../apps/commands.md).

## State, options & layout flags

The `ushort` flag words become named booleans, reached through the
[`Context`](../api/tvision_rs/view/struct.Context.html) and a view's `ViewState`.

| C++ Turbo Vision | tvision-rs (Rust) |
| ---------------- | -------------- |
| `state & sfFocused` | `self.state().focused` / [`StateFlag::Focused`](../api/tvision_rs/view/enum.StateFlag.html) |
| `options & ofSelectable` | `self.state().options.selectable` ([`Options`](../api/tvision_rs/view/struct.Options.html)) |
| `growMode` / `dragMode` | [`GrowMode`](../api/tvision_rs/view/struct.GrowMode.html) / [`DragMode`](../api/tvision_rs/view/struct.DragMode.html) |
| `helpCtx` / `hcNoContext` | `ViewState.help_ctx` / [`HelpCtx::NO_CONTEXT`](../api/tvision_rs/help/struct.HelpCtx.html) |
| `owner` / `current` / `selected` | [`ViewId`](../api/tvision_rs/view/struct.ViewId.html) handles |

The flag-word translation is detailed in
[Flag words → struct-of-bools](../port/flags.md), the handle model in
[Pointers & infoPtr → handles](../port/handles.md).

## Color, drawing & backend

| C++ Turbo Vision | tvision-rs (Rust) | notes |
| ---------------- | -------------- | ----- |
| `getColor` / `getPalette` | `ctx.theme.style(Role::…)` | [`Role`](../api/tvision_rs/theme/enum.Role.html) / [`Theme`](../api/tvision_rs/theme/struct.Theme.html) |
| `TColorAttr` | [`Style`](../api/tvision_rs/color/struct.Style.html) | |
| `TColorDesired` | [`Color`](../api/tvision_rs/color/enum.Color.html) | 4-variant enum |
| hardcoded glyph tables | fields on `theme::Glyphs`, via `ctx.glyphs()` | |
| `TDrawBuffer` | [`DrawBuffer`](../api/tvision_rs/screen/struct.DrawBuffer.html) | |
| `THardwareInfo` / `TScreen` | [`Backend`](../api/tvision_rs/backend/trait.Backend.html) trait | [`CrosstermBackend`](../api/tvision_rs/backend/struct.CrosstermBackend.html) / [`HeadlessBackend`](../api/tvision_rs/backend/struct.HeadlessBackend.html) |

See [Palettes & glyphs → Theme/Role](../port/theme.md),
[The draw model](../port/draw.md), and [Drawing & backends](../internals/drawing.md).

## Modal flow & data

| C++ Turbo Vision | tvision-rs (Rust) | notes |
| ---------------- | -------------- | ----- |
| `execView` (from a `Program`/`Application` method) | [`Program::exec_view_with`](../api/tvision_rs/app/struct.Program.html#method.exec_view_with) | result returned by value from an `extract` closure |
| `execView` (from within a view's `handleEvent`) | `Context::request_exec_view` | queues `Deferred::OpenModal`; close command routed to `requester` via `View::set_modal_answer` |
| `dragView` / press-tracking | capture-stack handlers | see [The event loop](../internals/event-loop.md) |
| `getData` / `setData` / `dataSize` | typed `value` / `set_value` | currency is [`FieldValue`](../api/tvision_rs/data/enum.FieldValue.html) |

The modal model is in [Modal execView → one loop + capture](../port/modal.md),
the data protocol in [Dialogs & data](../apps/dialogs.md).

## Dropped or replaced

| C++ Turbo Vision | tvision-rs (Rust) |
| ---------------- | -------------- |
| `drawHide` / `drawShow` / `drawUnder*` / buffered group | dropped — whole-tree redraw + diff |
| `TStreamable` / `TResourceFile` | dropped (serde if revived) |
| `forEach` / `firstThat` / `TSortedCollection` | iterators / `Vec<T: Ord>` |

Rationale for each removal is in
[Dropped & changed](../port/dropped.md).

---

> Anything not in this table ports verbatim — same name (minus the `T`), same
> method, same behaviour. For the differences that *do* change a symbol, see
> [Differences from C++ Turbo Vision](deviations.md).
