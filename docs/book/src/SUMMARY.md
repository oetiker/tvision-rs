# Summary

[Introduction](README.md)

# Getting Started

- [Installation & the `tv::` alias](getting-started/installation.md)
- [Your first app](getting-started/first-app.md)
- [The application skeleton](getting-started/skeleton.md)

# The Idiomatic Port (for Turbo Vision veterans)

- [What "faithful" means](port/faithful.md)
- [Inheritance → trait + composition](port/inheritance.md)
- [Pointers & infoPtr → handles](port/handles.md)
- [Events → enum + match](port/events.md)
- [Flag words → struct-of-bools](port/flags.md)
- [Constant families → open newtypes](port/constants.md)
- [Palettes & glyphs → Theme/Role](port/theme.md)
- [The draw model → whole-tree redraw + diff](port/draw.md)
- [Modal execView → one loop + capture](port/modal.md)
- [The Deferred channel](port/deferred.md)
- [Dropped & changed](port/dropped.md)

# Building Apps

- [Windows & the desktop](apps/windows.md)
- [Dialogs & data](apps/dialogs.md)
- [Controls](apps/controls.md)
- [Menus, status line & help](apps/menus.md)
- [Commands & events](apps/commands.md)
- [Keyboard & key mapping](apps/keyboard.md)
- [Theming & colors](apps/theming.md)
- [Text editing](apps/text-editing.md)

# How It Works

- [The view tree](internals/view-tree.md)
- [The event loop in depth](internals/event-loop.md)
- [Deferred effects](internals/deferred.md)
- [Cross-view brokering & ViewId](internals/brokering.md)
- [Drawing & backends](internals/drawing.md)
- [Writing your own View](internals/custom-view.md)

# Reference

- [How the API docs are organized](reference/api.md)
- [C++ Turbo Vision → tvision symbol map](reference/symbol-map.md)
- [Deviations D1–D13](reference/deviations.md)
- [The screenshot tooling](reference/screenshots.md)
