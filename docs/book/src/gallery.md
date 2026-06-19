# Widget gallery

Every widget tvision-rs draws, shown running with the code that builds it. Each
screenshot is the **real terminal output** of the
[`gallery` example](https://github.com/oetiker/tvision-rs/blob/main/examples/gallery.rs),
captured live; each code block is the *same* builder fn the screenshot was made
from, included verbatim — so every line you see here compiles.

> Run any of these yourself:
> ```console
> $ cargo run --example gallery -- <name>     # e.g. button, listbox, filedialog
> $ cargo run --example gallery               # list every name
> ```
> Each builder returns a `Box<dyn View>` (or a `Menu` / status definition); the
> example wraps it in a desktop with a menu bar and status line and runs the real
> event loop. See [Controls](apps/controls.md), [Dialogs & data](apps/dialogs.md),
> and [Menus, status line & help](apps/menus.md) for the concepts behind them.

## Buttons & selection

### Button

A clickable command button — the `~` marks the hot-letter; `default: true` makes
`Enter` fire it.

{{#include screens/button.html}}

```rust,ignore
{{#rustdoc_include ../../../examples/gallery.rs:button}}
```

### Check boxes

Independent on/off boxes; any combination may be set. `cluster.value` is a
bitmask.

{{#include screens/checkboxes.html}}

```rust,ignore
{{#rustdoc_include ../../../examples/gallery.rs:checkboxes}}
```

### Radio buttons

Mutually-exclusive options sharing one cluster; `cluster.value` is the selected
index.

{{#include screens/radiobuttons.html}}

```rust,ignore
{{#rustdoc_include ../../../examples/gallery.rs:radiobuttons}}
```

## Text entry & labels

### Input line

A single-line text field with a linked [`Label`](apps/controls.md) — the label's
hot-letter focuses the field.

{{#include screens/inputline.html}}

```rust,ignore
{{#rustdoc_include ../../../examples/gallery.rs:inputline}}
```

### Input line with history

An input line plus a `THistory` dropdown icon that recalls earlier entries from a
named channel.

{{#include screens/history.html}}

```rust,ignore
{{#rustdoc_include ../../../examples/gallery.rs:history}}
```

### Static text

Read-only text with word-wrap; a leading `\x03` centers a line.

{{#include screens/statictext.html}}

```rust,ignore
{{#rustdoc_include ../../../examples/gallery.rs:statictext}}
```

## Lists & trees

### List box

A scrollable single-column list wired to a vertical scroll bar. Its items are
filled on the first event tick, because `new_list` needs a `Context` — the thin
wrapper view is the idiomatic deferred-init pattern.

{{#include screens/listbox.html}}

```rust,ignore
{{#rustdoc_include ../../../examples/gallery.rs:listbox}}
```

### Outline

A collapsible tree viewer built from a `Node` tree, with horizontal and vertical
scroll bars. Build the tree with `Node::new` and the builder methods, then pass
the root to `Outline::new`. Each node can be expanded or collapsed with a mouse
click on the `+`/`-` icon or the `Enter` key. The underlying `OutlineViewer`
trait lets you swap in a custom data source (your own `get_root`/`get_next`/
`get_child`/`get_text`/`is_expanded` implementation) without reimplementing draw
or navigation logic.

Call `ov_update(outline, ctx)` once after insertion (needs a `Context`) to
publish the scroll range and initial cursor position to the sibling scroll bars.

{{#include screens/outline.html}}

```rust,ignore
{{#rustdoc_include ../../../examples/gallery.rs:outline}}
```

## Scrolling & display

### Scroll bars

A vertical and a horizontal scroll bar; the thumb position comes from the public
`value` / `max_value` / `page_step` fields.

{{#include screens/scrollbar.html}}

```rust,ignore
{{#rustdoc_include ../../../examples/gallery.rs:scrollbar}}
```

### Memo

A multi-line text editor for use inside a dialog; `set_text` loads its initial
content.

{{#include screens/memo.html}}

```rust,ignore
{{#rustdoc_include ../../../examples/gallery.rs:memo}}
```

### Terminal

A scrolling output view you *write into*. `Terminal` embeds a `Scroller` and
stores incoming bytes in a fixed-size ring buffer; the most recent lines that fit
the view height are drawn. Write to it via the `TextDevice::write_bytes` method —
there is no stream wrapper. Call `Terminal::init` once after insertion (needs a
`Context`) to set the scroll limit, cursor position, and visibility. The gallery
wrapper view seeds both calls on the first event tick because a `Context` is not
available at construction time.

{{#include screens/terminal.html}}

```rust,ignore
{{#rustdoc_include ../../../examples/gallery.rs:terminal}}
```

## Windows & dialogs

### Window

A plain titled, movable, resizable window that hosts child views.

{{#include screens/window.html}}

```rust,ignore
{{#rustdoc_include ../../../examples/gallery.rs:window}}
```

### Splitter

Resizable multi-pane layout. `Splitter::cols()`/`rows()` arrange panes along one
axis with draggable divider seams; `.joined()` connects the divider lines to the
window frame (`┬ ┴ ┤`) and to each other (`├`). The left column here is a fixed
sidebar; the right column is split into two rows.

{{#include screens/splitter.html}}

```rust,ignore
{{#rustdoc_include ../../../examples/gallery.rs:splitter}}
```

### Dialog

A modal form combining a labelled input, check boxes, and `OK` / `Cancel`
buttons — the canonical Turbo Vision dialog.

{{#include screens/dialog.html}}

```rust,ignore
{{#rustdoc_include ../../../examples/gallery.rs:dialog}}
```

### Message box

A short message with a default button. (The framework's `Program::message_box`
helper builds one for you; this shows the equivalent assembled by hand.)

{{#include screens/messagebox.html}}

```rust,ignore
{{#rustdoc_include ../../../examples/gallery.rs:messagebox}}
```

### Color picker

The truecolor picker dialog — a four-tab RGB / HSV / palette surface.

{{#include screens/colorpicker.html}}

```rust,ignore
{{#rustdoc_include ../../../examples/gallery.rs:colorpicker}}
```

### File dialog

A file-open dialog: a name field, the file and directory lists, and action
buttons. The lists fill themselves when the dialog is run modally.

{{#include screens/filedialog.html}}

```rust,ignore
{{#rustdoc_include ../../../examples/gallery.rs:filedialog}}
```

### Change-directory dialog

A directory chooser with a navigable tree.

{{#include screens/chdirdialog.html}}

```rust,ignore
{{#rustdoc_include ../../../examples/gallery.rs:chdirdialog}}
```

### Editor window

A ready-made editor window — a file-backed editor wired to scroll bars and a
line:column indicator.

{{#include screens/editor.html}}

```rust,ignore
{{#rustdoc_include ../../../examples/gallery.rs:editor}}
```

## Application chrome

### Menu bar

The top menu bar, built from nested pull-downs with hot-keys and accelerators.
`File ▸ Recent` is a sub-menu nested inside a sub-menu.

{{#include screens/menubar.html}}

```rust,ignore
{{#rustdoc_include ../../../examples/gallery.rs:menubar}}
```

### Context menu

A right-click pop-up, built from the same `Menu` data and wrapped in a `MenuBox`.

{{#include screens/contextmenu.html}}

```rust,ignore
{{#rustdoc_include ../../../examples/gallery.rs:contextmenu}}
```

### Status line

The bottom status line of labelled hot-key items; each fires a command when
clicked or keyed.

{{#include screens/statusline.html}}

```rust,ignore
{{#rustdoc_include ../../../examples/gallery.rs:statusline}}
```
