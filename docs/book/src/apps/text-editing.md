# Text editing

tvision ships a full multi-line text editor and a scrolling terminal view, both
ported faithfully from Turbo Vision. They build on one engine,
[`Editor`](../api/tvision/widgets/struct.Editor.html) (`TEditor`): a gap-buffer
text editor with a cursor, a selection, single-level undo, word-by-word
navigation, and substring search. You rarely embed `Editor` directly — you
reach for one of the three faces it wears.

## Three faces of the editor

| Type | C++ | Use it for |
| ---- | --- | ---------- |
| [`Memo`](../api/tvision/widgets/struct.Memo.html) | `TMemo` | a multi-line edit field *inside a dialog* |
| [`FileEditor`](../api/tvision/widgets/struct.FileEditor.html) | `TFileEditor` | an editor backed by a file on disk |
| [`EditWindow`](../api/tvision/widgets/struct.EditWindow.html) | `TEditWindow` | a ready-made window wrapping a `FileEditor` plus its scrollbars and indicator |

[`Memo`](../api/tvision/widgets/struct.Memo.html) is a thin wrapper over
`Editor` that does two extra things: it lets a plain <kbd>Tab</kbd> fall through
to the dialog's focus navigation (instead of inserting a tab), and it exposes
its text as a typed `FieldValue` so dialog [gather/scatter](dialogs.md) works
just like any other control.

[`EditWindow`](../api/tvision/widgets/struct.EditWindow.html) is the one to
start from for an editor application. Its constructor inserts the (initially
hidden) horizontal and vertical scrollbars and the line/column indicator, wires
a [`FileEditor`](../api/tvision/widgets/struct.FileEditor.html) to them, and
titles the window after the file (or `"Untitled"`):

```rust,ignore
let win = EditWindow::new(bounds, Some(path), window_number);
desktop.insert(Box::new(win));
```

## Files: loading, saving, and the modified prompt

[`FileEditor`](../api/tvision/widgets/struct.FileEditor.html) loads its file in
the constructor when you pass a path; a `None` path is an *untitled* buffer. It
handles `Command::SAVE` itself, opens a Save-as file dialog for an untitled
buffer or `Command::SAVE_AS`, and — through its `valid` check — puts up the
classic *"… has been modified. Save?"* Yes/No/Cancel prompt before a dirty
buffer is closed. All of that runs through tvision's deferred message-box and
file-dialog seams, so a single editor view can drive a modal dialog without
owning the event loop (see [Deferred effects](../internals/deferred.md)).

Backup files are off by default; when enabled, tvision appends `~` to the
filename (`foo.txt` → `foo.txt~`), following Unix convention rather than the DOS
`.bak` rename.

## Line endings and encoding

Two settings control how text is stored and stepped over:

- [`LineEnding`](../api/tvision/widgets/enum.LineEnding.html) — `Lf`, `CrLf`, or
  `Cr`, deciding the byte sequence written for each line break when text is
  inserted. tvision defaults to `Lf` (the modern-host default; DOS Turbo Vision
  defaulted to `CrLf`).
- [`Encoding`](../api/tvision/widgets/enum.Encoding.html) — `Default` steps over
  characters using width-aware (grapheme) logic, so multi-byte UTF-8 and wide
  glyphs advance the cursor correctly; `SingleByte` treats every byte as one
  column.

## The terminal view

[`Terminal`](../api/tvision/widgets/terminal/struct.Terminal.html) (`TTerminal`)
is the other text view: a scrolling, ring-buffered output pane. It is not an
editor — you *write into* it. It implements the
[`TextDevice`](../api/tvision/widgets/terminal/trait.TextDevice.html) trait, so
you append output by calling
[`write_bytes`](../api/tvision/widgets/terminal/trait.TextDevice.html#method.write_bytes);
the most recent lines that fit are drawn, and the embedded scroller keeps its
scrollbars in sync. The C++ `streambuf`/`otstream` plumbing is dropped — there is
no stream wrapper, just the byte sink.

Because its constructor cannot touch the screen, `Terminal` follows the
deferred-init pattern: build it, insert it into a group, then call
[`init`](../api/tvision/widgets/terminal/struct.Terminal.html#method.init) once
to set up its limits and cursor.

## Where to go next

- [Dialogs & data](dialogs.md) — how a `Memo`'s text flows through
  gather/scatter.
- [Windows & the desktop](windows.md) — placing and tiling editor windows.
- [Deferred effects](../internals/deferred.md) — why a view can request a modal
  save prompt without owning the loop.
