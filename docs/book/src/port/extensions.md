# Extensions beyond Turbo Vision

This page lists the widgets, options and subsystems tvision-rs adds to what
magiblot/tvision offers. Convenience methods that wrap what a Turbo Vision
program does by hand, such as `Program::open_file_dialog`, are not listed.
Where the guide describes one of these features, a
**tvision-rs extension** callout at the start of the section points back here.

## New widgets

These have no Turbo Vision counterpart.

- [`Table`](../api/tvision_rs/widgets/table/struct.Table.html): a sortable
  multi-column list with a click-to-sort header, columns the user can resize,
  move and hide, and in-place cell editing. See the
  [gallery](../gallery.md#table) and [Controls](../apps/controls.md#list-boxes-scrollers--scroll-bars).
- [`Splitter`](../api/tvision_rs/widgets/splitter/struct.Splitter.html): a
  resizable multi-pane layout with draggable dividers, a keyboard resize mode
  and divider lines that can join the window frame
  ([`junction`](../api/tvision_rs/junction/index.html)). See
  [Windows & the desktop](../apps/windows.md#splitter-resizable-panes) and the
  [gallery](../gallery.md#splitter).
- [`ColorPicker`](../api/tvision_rs/dialog/struct.ColorPicker.html) and
  [`Program::color_dialog`](../api/tvision_rs/app/struct.Program.html#method.color_dialog):
  a truecolor picker with Presets, RGB, Hue/Sat and Xterm tabs, either
  embedded in a dialog or run as a ready-made modal that returns the chosen
  color. See the [gallery](../gallery.md#color-picker).
- [`MaskedInput`](../api/tvision_rs/widgets/struct.MaskedInput.html) and
  [`RevealEye`](../api/tvision_rs/widgets/struct.RevealEye.html): a password
  field that shows bullets, with a one-cell eye that reveals the text while held
  or for a timed peek. See the [gallery](../gallery.md#masked-input).
- [`TabBar`](../api/tvision_rs/widgets/struct.TabBar.html) and
  [`PageStack`](../api/tvision_rs/widgets/struct.PageStack.html): a one-row tab
  selector and a container that shows one page at a time, coupled so the bar
  switches the page. The color picker is built from them.
- [`Program::theme_editor`](../api/tvision_rs/app/struct.Program.html#method.theme_editor):
  a dialog that lists every theme role, edits its colors with the color picker
  and installs the result on **OK**. See
  [Theming & colors](../apps/theming.md#the-theme-editor).
- [`RegexValidator`](../api/tvision_rs/validate/struct.RegexValidator.html): an
  input-line validator driven by a regular expression, with the same
  type-as-you-go checking as the picture-mask validator. See
  [Controls](../apps/controls.md#validators).

`TColorDialog` and its helper classes are not ported: they edited the
application's `TPalette`, each entry a foreground/background attribute built
from the BIOS colors, which tvision-rs replaces with a role-based theme.
[`ColorPicker`](../api/tvision_rs/dialog/struct.ColorPicker.html) takes their
place; see [Dropped & changed](dropped.md#tcolordialog-replaced-by-colorpicker).

## New options on ported widgets

These extend a ported Turbo Vision class.

- [`FindMode`](../api/tvision_rs/widgets/list_viewer/enum.FindMode.html)
  (`TListViewer`): incremental find on list boxes. Typing builds a query that
  highlights the matched text and can filter the list to the matching rows. See
  [Controls](../apps/controls.md#list-boxes-scrollers--scroll-bars) and the
  [gallery](../gallery.md#list-box).
- [`InputLine::set_read_only`](../api/tvision_rs/widgets/struct.InputLine.html#method.set_read_only)
  (`TInputLine`): a field that refuses edits but still takes focus, scrolls and
  copies. See [Controls](../apps/controls.md#read-only-fields).
- [`InputLine::set_value_position`](../api/tvision_rs/widgets/struct.InputLine.html#method.set_value_position)
  (`TInputLine`): shows the start of a value too long for the field instead of
  its end. See [Controls](../apps/controls.md#which-end-of-a-long-value-shows).
- [`InputLine::set_select_all_on_focus`](../api/tvision_rs/widgets/struct.InputLine.html#method.set_select_all_on_focus)
  (`TInputLine`): keeps the caret and selection when the field gains focus
  instead of selecting the whole text.
- [`InputLine::set_mask`](../api/tvision_rs/widgets/struct.InputLine.html#method.set_mask)
  and [`set_reveal`](../api/tvision_rs/widgets/struct.InputLine.html#method.set_reveal)
  (`TInputLine`): draw every character as an echo character, for passwords.
  `MaskedInput` builds on them.
- [`Fullscreen`](../api/tvision_rs/window/enum.Fullscreen.html) and
  [`Window::set_fullscreen`](../api/tvision_rs/window/struct.Window.html#method.set_fullscreen)
  (`TWindow`): a frameless window that fills the desktop or the whole screen,
  with the menu bar collapsed to a `[⋮]` button. See
  [Windows & the desktop](../apps/windows.md#fullscreen-windows).
- [`Dialog::button_row`](../api/tvision_rs/dialog/struct.Dialog.html#method.button_row)
  and [`ButtonLayout`](../api/tvision_rs/dialog/enum.ButtonLayout.html)
  (`TDialog`): lay out a row of buttons with the classic metrics, with face
  widths that are fixed, uniform or sized to each label.
- [`Group::set_surface`](../api/tvision_rs/view/struct.Group.html#method.set_surface)
  (`TGroup`): fills a group's background with a role that changes when the pane
  gains or loses focus.

## New subsystems

- [`Keymap`](../api/tvision_rs/keymap/struct.Keymap.html): the keys of the
  editor and the input line are a configurable table of chords, with WordStar,
  CUA and Emacs presets. See
  [Keyboard & key mapping](../apps/keyboard.md#the-keymap) and
  [Text editing](../apps/text-editing.md#key-bindings).
- [`Backend::get_primary`](../api/tvision_rs/backend/trait.Backend.html#method.get_primary)
  and [`set_primary`](../api/tvision_rs/backend/trait.Backend.html#method.set_primary):
  middle-click pastes the X11 or Wayland PRIMARY selection into an input line
  or the editor, and a mouse selection in an input line becomes the PRIMARY
  selection for other programs.
- [`HeadlessBackend`](../api/tvision_rs/backend/struct.HeadlessBackend.html)
  and [`screen::snapshot`](../api/tvision_rs/screen/snapshot/index.html): an
  in-memory terminal with injected input and an inspectable screen, and a
  stable text format of that screen for snapshot tests, for testing an
  application without a terminal.
- [`CrosstermBackend`](../api/tvision_rs/backend/struct.CrosstermBackend.html)
  wraps every frame in synchronized output (DEC mode 2026), so terminals that
  support it show each frame whole.
