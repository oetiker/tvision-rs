# Widget gallery

Every widget rstv draws, shown running with the code that builds it. Each
screenshot is the **real terminal output** of the
[`gallery` example](https://github.com/oetiker/rstv/blob/main/examples/gallery.rs),
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
scroll bars.

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

A scrolling output view you *write into*. `init` and `write_bytes` need a
`Context`, so the wrapper seeds it on the first event tick.

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

{{#include screens/menubar.html}}

```rust,ignore
{{#rustdoc_include ../../../examples/gallery.rs:menubar}}
```

### Status line

The bottom status line of labelled hot-key items; each fires a command when
clicked or keyed.

{{#include screens/statusline.html}}

```rust,ignore
{{#rustdoc_include ../../../examples/gallery.rs:statusline}}
```
