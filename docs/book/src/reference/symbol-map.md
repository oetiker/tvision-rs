# C++ Turbo Vision → tvision symbol map

A terse lookup for translating a C++ Turbo Vision symbol into its tvision
equivalent. This is the *what*; the *why* lives in
[The Idiomatic Port](../port/faithful.md) and the deviation summary in
[Deviations D1–D13](deviations.md).

Two mechanical rules cover most of the table:

- **Drop the `T` prefix and namespace under `tv::`** — `TButton` → `tv::Button`.
- **`cmFoo` / `hcFoo` constant families** become associated consts on an open
  newtype — `cmOK` → [`tv::Command::OK`](../api/tvision/command/struct.Command.html),
  `hcNoContext` → [`tv::HelpCtx::NO_CONTEXT`](../api/tvision/help/struct.HelpCtx.html).

## Core types

| C++ Turbo Vision | tvision (Rust) | notes |
| ---------------- | -------------- | ----- |
| `TView` | [`View`](../api/tvision/view/trait.View.html) (trait) + [`ViewState`](../api/tvision/view/struct.ViewState.html) (data) | behaviour is a trait; data is composed in, not inherited |
| `TGroup` | [`Group`](../api/tvision/view/struct.Group.html) | owns `Vec<Box<dyn View>>` |
| `TFrame` | [`Frame`](../api/tvision/frame/struct.Frame.html) | |
| `TWindow` | [`Window`](../api/tvision/window/struct.Window.html) | |
| `TDialog` | [`Dialog`](../api/tvision/dialog/struct.Dialog.html) | |
| `TDeskTop` | [`Desktop`](../api/tvision/desktop/struct.Desktop.html) | |
| `TProgram` | [`Program`](../api/tvision/app/struct.Program.html) | the engine: tree + event loop + backend |
| `TApplication` | [`Application`](../api/tvision/app/struct.Application.html) | thin wrapper over `Program` |

See [The application skeleton](../getting-started/skeleton.md) for the
`Program` / `Application` split.

## Controls & widgets

| C++ Turbo Vision | tvision (Rust) |
| ---------------- | -------------- |
| `TButton` | [`Button`](../api/tvision/widgets/struct.Button.html) |
| `TStaticText` | [`StaticText`](../api/tvision/widgets/struct.StaticText.html) |
| `TParamText` | [`ParamText`](../api/tvision/widgets/struct.ParamText.html) |
| `TLabel` | [`Label`](../api/tvision/widgets/struct.Label.html) |
| `TInputLine` | [`InputLine`](../api/tvision/widgets/struct.InputLine.html) |
| `TCluster` | [`Cluster`](../api/tvision/widgets/struct.Cluster.html) |
| `TCheckBoxes` | [`CheckBoxes`](../api/tvision/widgets/struct.CheckBoxes.html) |
| `TRadioButtons` | [`RadioButtons`](../api/tvision/widgets/struct.RadioButtons.html) |
| `TScrollBar` | [`ScrollBar`](../api/tvision/widgets/struct.ScrollBar.html) |
| `TScroller` | [`Scroller`](../api/tvision/widgets/struct.Scroller.html) |
| `TListViewer` | [`ListViewer`](../api/tvision/widgets/list_viewer/trait.ListViewer.html) (trait) |
| `TListBox` | [`ListBox`](../api/tvision/widgets/struct.ListBox.html) |
| `TOutline` | [`Outline`](../api/tvision/widgets/outline/struct.Outline.html) / [`OutlineViewer`](../api/tvision/widgets/outline/trait.OutlineViewer.html) |
| `TEditor` | [`Editor`](../api/tvision/widgets/struct.Editor.html) |
| `TEditWindow` | [`EditWindow`](../api/tvision/widgets/struct.EditWindow.html) |
| `TMenuBar` | [`MenuBar`](../api/tvision/menu/menu_bar/struct.MenuBar.html) |
| `TMenu` / `TMenuItem` | [`Menu`](../api/tvision/menu/struct.Menu.html) / [`MenuItem`](../api/tvision/menu/enum.MenuItem.html) |
| `TStatusLine` / `TStatusItem` | [`StatusLine`](../api/tvision/status/status_line/struct.StatusLine.html) / [`StatusItem`](../api/tvision/status/struct.StatusItem.html) |
| `TValidator` family | [`Validator`](../api/tvision/validate/trait.Validator.html) trait + impls |

The full set is in [Controls](../apps/controls.md) and
[Menus, status line & help](../apps/menus.md).

## Events, keys & commands

| C++ Turbo Vision | tvision (Rust) | notes |
| ---------------- | -------------- | ----- |
| `TEvent` / `event.what == evX` | [`Event`](../api/tvision/event/enum.Event.html) enum, matched | `evKeyDown` → `Event::KeyDown(..)`, `evCommand` → `Event::Command(..)` |
| `KeyDownEvent` | [`KeyEvent`](../api/tvision/event/struct.KeyEvent.html) | |
| `MouseEventType` | [`MouseEvent`](../api/tvision/event/struct.MouseEvent.html) | |
| `kbEnter`, `kbF1`, … | [`Key`](../api/tvision/event/enum.Key.html) enum (`Key::Enter`, `Key::F(1)`) | combined codes decompose |
| `kbCtrlC`, `kbShiftTab` | base `Key` + [`KeyModifiers`](../api/tvision/event/struct.KeyModifiers.html) | `Key::Char('c')` + ctrl |
| `clearEvent(event)` | `*ev = Event::Nothing` | |
| `cmOK`, `cmCancel`, … | [`Command`](../api/tvision/command/struct.Command.html) assoc consts | open newtype, namespaced |
| `TCommandSet` (256-bit) | [`CommandSet`](../api/tvision/command/struct.CommandSet.html) | `Program` stores the *disabled* set (denylist) |
| `message(rcvr, evBroadcast, cmX, p)` | `ctx.broadcast(Command::X)` | |
| `message(...)` expecting a result | targeted query → `Option<T>` | |

How events route is covered in [Events → enum + match](../port/events.md) and
[Commands & events](../apps/commands.md).

## State, options & layout flags

The `ushort` flag words become named booleans, reached through the
[`Context`](../api/tvision/view/struct.Context.html) and a view's `ViewState`.

| C++ Turbo Vision | tvision (Rust) |
| ---------------- | -------------- |
| `state & sfFocused` | `self.state().focused` / [`StateFlag::Focused`](../api/tvision/view/enum.StateFlag.html) |
| `options & ofSelectable` | `self.state().options.selectable` ([`Options`](../api/tvision/view/struct.Options.html)) |
| `growMode` / `dragMode` | [`GrowMode`](../api/tvision/view/struct.GrowMode.html) / [`DragMode`](../api/tvision/view/struct.DragMode.html) |
| `helpCtx` / `hcNoContext` | `ViewState.help_ctx` / [`HelpCtx::NO_CONTEXT`](../api/tvision/help/struct.HelpCtx.html) |
| `owner` / `current` / `selected` | [`ViewId`](../api/tvision/view/struct.ViewId.html) handles |

The flag-word translation is detailed in
[Flag words → struct-of-bools](../port/flags.md), the handle model in
[Pointers & infoPtr → handles](../port/handles.md).

## Color, drawing & backend

| C++ Turbo Vision | tvision (Rust) | notes |
| ---------------- | -------------- | ----- |
| `getColor` / `getPalette` | `ctx.theme.style(Role::…)` | [`Role`](../api/tvision/theme/enum.Role.html) / [`Theme`](../api/tvision/theme/struct.Theme.html) |
| `TColorAttr` | [`Style`](../api/tvision/color/struct.Style.html) | |
| `TColorDesired` | [`Color`](../api/tvision/color/enum.Color.html) | 4-variant enum |
| hardcoded glyph tables | fields on `theme::Glyphs`, via `ctx.glyphs()` | |
| `TDrawBuffer` | [`DrawBuffer`](../api/tvision/screen/struct.DrawBuffer.html) | |
| `THardwareInfo` / `TScreen` | [`Backend`](../api/tvision/backend/trait.Backend.html) trait | [`CrosstermBackend`](../api/tvision/backend/struct.CrosstermBackend.html) / [`HeadlessBackend`](../api/tvision/backend/struct.HeadlessBackend.html) |

See [Palettes & glyphs → Theme/Role](../port/theme.md),
[The draw model](../port/draw.md), and [Drawing & backends](../internals/drawing.md).

## Modal flow & data

| C++ Turbo Vision | tvision (Rust) | notes |
| ---------------- | -------------- | ----- |
| `execView` | `exec_view` | result returned via a posted `Command` |
| `dragView` / press-tracking | capture-stack handlers | see [The event loop](../internals/event-loop.md) |
| `getData` / `setData` / `dataSize` | typed `value` / `set_value` | currency is [`FieldValue`](../api/tvision/data/enum.FieldValue.html) |

The modal model is in [Modal execView → one loop + capture](../port/modal.md),
the data protocol in [Dialogs & data](../apps/dialogs.md).

## Dropped or replaced

| C++ Turbo Vision | tvision (Rust) |
| ---------------- | -------------- |
| `drawHide` / `drawShow` / `drawUnder*` / buffered group | dropped — whole-tree redraw + diff |
| `TStreamable` / `TResourceFile` | dropped (serde if revived) |
| `forEach` / `firstThat` / `TSortedCollection` | iterators / `Vec<T: Ord>` |

Rationale for each removal is in
[Dropped & changed](../port/dropped.md).

---

> Anything not in this table ports verbatim — same name (minus the `T`), same
> method, same behaviour. When in doubt, the full deviation lookup is
> Appendix A of `docs/PORTING-GUIDE.md`.
