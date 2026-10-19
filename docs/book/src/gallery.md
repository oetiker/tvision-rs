# Widget gallery

Every widget tvision-rs draws, shown running with the code that builds it. Each
screenshot is the **real terminal output** of the
[`gallery` example](https://github.com/oetiker/tvision-rs/blob/main/examples/gallery.rs),
captured live. Each code block is the builder fn the screenshot was made from,
included verbatim from the compiled example.

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

A clickable command button. The `~` marks the hot-letter; `default: true` makes
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

A single-line text field with a linked [`Label`](apps/controls.md). The label's
hot-letter focuses the field.

{{#include screens/inputline.html}}

```rust,ignore
{{#rustdoc_include ../../../examples/gallery.rs:inputline}}
```

### Masked input

> **tvision-rs extension**: not in Turbo Vision. See
> [Extensions beyond Turbo Vision](port/extensions.md).

A password field ([`MaskedInput`](apps/controls.md)) that shows its value as
bullets while keeping the real text intact, with a reveal eye in the last column
(its own Tab stop). Press-and-hold the eye, or focus it and press Space, to peek;
Cut/Copy never place the cleartext on the clipboard. The eye glyphs come from the
theme.

{{#include screens/masked_input.html}}

```rust,ignore
{{#rustdoc_include ../../../examples/gallery.rs:masked_input}}
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
filled on the first event tick, because `new_list` needs a `Context`; the thin
wrapper view is the deferred-init pattern.

> **tvision-rs extension** to `TListViewer`: incremental find is not in Turbo
> Vision. See [Extensions beyond Turbo Vision](port/extensions.md).

Opt-in incremental find
(`with_find(FindMode::Filter)` or `FindMode::Highlight`) lets the list own its
query and highlight or self-filter while focused (see
[Controls](apps/controls.md)).

{{#include screens/listbox.html}}

```rust,ignore
{{#rustdoc_include ../../../examples/gallery.rs:listbox}}
```

### Table

> **tvision-rs extension**: not in Turbo Vision. See
> [Extensions beyond Turbo Vision](port/extensions.md).

A column header over a scrolling row list. Rows come from a `TableModel` (`VecModel` for plain
strings) and carry a stable `RowKey`, so focus and the marked rows follow the
record through a re-sort or a `set_model` reload. Click a column label to sort
by it, click again to flip; numbers sort numerically. Columns shrink widest
first to fit (never below `min_width`), cut text with `…`, and can be hidden;
a `flex` column takes the spare width when the table is wider than its
columns. Columns that do not fit at their `min_width` scroll horizontally:
←/→ or the horizontal bar (`with_h_bar`) move them, the header moves along,
the check column stays put. Resize the window in the demo to see both.
Drag the separator right of a label to resize that column (it then keeps that
width), drag a label to move the column (a marker shows where it lands), and
right-click the header to show or hide columns from a popup; `TABLE_COLUMNS`
opens the same popup from a key or menu. Each of these broadcasts
`TABLE_COLUMNS_CHANGED`; `column_layout()` / `set_column_layout()` save and
restore the arrangement. `resizable(false)` / `movable(false)` lock a column.
`with_separators` decides whether the one-cell gaps between the columns stay
blank (the default, `Separators::Off`) or carry the theme's `│` rule:
`Separators::Header` rules the label row only, so the separator you drag to
resize a column has a visible target, and `Separators::All` rules between every
row's cells as well. When the columns leave spare width, the header also rules
the cell after the last column, which is that column's resize handle; the rows
leave that cell blank.
In `SelectionMode::Multi` the rows get a leading check column drawn like
check boxes: click a row's `[ ]` box, or press Space, to mark it. The table
broadcasts `TABLE_SORT_CHANGED`, `TABLE_SELECTION_CHANGED` and, in single
mode, `LIST_ITEM_SELECTED`, each with the table's id as `source`; its value is
the focused key (single) or the list of marked keys (multi). Keyboard sorting
is opt-in: bind keys to `TABLE_SORT_NEXT` / `TABLE_SORT_FLIP`.
Columns built `editable(true)` are edited in place: the focused row shows a
cell cursor that ←/→ move between them, and Enter, F2 or a double-click opens
an input line over the cell (with the column's `validator`, if any). Enter
stores the text through `TableModel::set_cell` (a refusal shows its message
and keeps the editor open) and leaves the cursor on the cell, Esc or a click
elsewhere cancels, Tab/Shift-Tab store and move to the next/previous editable
cell; a store re-sorts and broadcasts `TABLE_CELL_EDITED`. Without an editable
column Enter keeps its list meaning and presses the dialog's default button.
In the demo, Image and CPU % are editable.

{{#include screens/table.html}}

```rust,ignore
{{#rustdoc_include ../../../examples/gallery.rs:table}}
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
the view height are drawn. Write to it via the `TextDevice::write_bytes` method;
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

A titled, movable, resizable window that hosts child views.

{{#include screens/window.html}}

```rust,ignore
{{#rustdoc_include ../../../examples/gallery.rs:window}}
```

### Splitter

> **tvision-rs extension**: not in Turbo Vision. See
> [Extensions beyond Turbo Vision](port/extensions.md).

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
buttons, the canonical Turbo Vision dialog.

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

> **tvision-rs extension**: not in Turbo Vision. See
> [Extensions beyond Turbo Vision](port/extensions.md).

The truecolor picker dialog, with Presets, RGB, Hue/Sat and Xterm tabs. It takes
the place of Turbo Vision's `TColorDialog`.

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

A ready-made editor window: a file-backed editor wired to scroll bars and a
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
