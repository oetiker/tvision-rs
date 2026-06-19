# Text editing

tvision-rs ships a full multi-line text editor and a scrolling terminal view, both
ported faithfully from Turbo Vision. They build on one engine,
[`Editor`](../api/tvision_rs/widgets/struct.Editor.html) (`TEditor`): a gap-buffer
text editor with a cursor, a selection, single-level undo, word-by-word
navigation, and substring search. You rarely embed `Editor` directly — you
reach for one of the three faces it wears.

## Three faces of the editor

| Type | Use it for |
| ---- | ---------- |
| [`Memo`](../api/tvision_rs/widgets/struct.Memo.html) *(`TMemo`)* | a multi-line edit field *inside a dialog* |
| [`FileEditor`](../api/tvision_rs/widgets/struct.FileEditor.html) *(`TFileEditor`)* | an editor backed by a file on disk |
| [`EditWindow`](../api/tvision_rs/widgets/struct.EditWindow.html) *(`TEditWindow`)* | a ready-made window wrapping a `FileEditor` plus its scrollbars and indicator |

[`Memo`](../api/tvision_rs/widgets/struct.Memo.html) is a thin wrapper over
`Editor` that does two extra things: it lets a plain <kbd>Tab</kbd> fall through
to the dialog's focus navigation (instead of inserting a tab), and it exposes
its text as a typed `FieldValue` so dialog [gather/scatter](dialogs.md) works
just like any other control.

[`EditWindow`](../api/tvision_rs/widgets/struct.EditWindow.html) is the one to
start from for an editor application. Its constructor inserts the (initially
hidden) horizontal and vertical scrollbars and the line/column indicator, wires
a [`FileEditor`](../api/tvision_rs/widgets/struct.FileEditor.html) to them, and
titles the window after the file (or `"Untitled"`):

```rust
# use tvision_rs as tv;
# use tv::widgets::EditWindow;
# fn _demo(desktop: &mut tv::Group) {
# let bounds = tv::Rect::new(0, 0, 80, 24);
# let path = std::path::PathBuf::from("my_file.txt");
# let window_number: i16 = 1;
let win = EditWindow::new(bounds, Some(path), window_number);
desktop.insert(Box::new(win));
# }
```

## Files: loading, saving, and the modified prompt

[`FileEditor`](../api/tvision_rs/widgets/struct.FileEditor.html) loads its file in
the constructor when you pass a path; a `None` path is an *untitled* buffer. It
handles `Command::SAVE` itself, opens a Save-as file dialog for an untitled
buffer or `Command::SAVE_AS`, and — through its `valid` check — puts up the
classic *"… has been modified. Save?"* Yes/No/Cancel prompt before a dirty
buffer is closed. All of that runs through tvision-rs's deferred message-box and
file-dialog seams, so a single editor view can drive a modal dialog without
owning the event loop (see [Deferred effects](../internals/deferred.md)).

Backup files are off by default; when enabled, tvision-rs appends `~` to the
filename (`foo.txt` → `foo.txt~`), following Unix convention rather than the DOS
`.bak` rename.

## Line endings and encoding

Two settings control how text is stored and stepped over:

- [`LineEnding`](../api/tvision_rs/widgets/enum.LineEnding.html) — `Lf`, `CrLf`, or
  `Cr`, deciding the byte sequence written for each line break when text is
  inserted. tvision-rs defaults to `Lf` (the modern-host default; DOS Turbo Vision
  defaulted to `CrLf`).
- [`Encoding`](../api/tvision_rs/widgets/enum.Encoding.html) — `Default` steps over
  characters using width-aware (grapheme) logic, so multi-byte UTF-8 and wide
  glyphs advance the cursor correctly; `SingleByte` treats every byte as one
  column.

## The terminal view

[`Terminal`](../api/tvision_rs/widgets/terminal/struct.Terminal.html) (`TTerminal`)
is the other text view: a scrolling, ring-buffered output pane. It is not an
editor — you *write into* it. It implements the
[`TextDevice`](../api/tvision_rs/widgets/terminal/trait.TextDevice.html) trait, so
you append output by calling
[`write_bytes`](../api/tvision_rs/widgets/terminal/trait.TextDevice.html#method.write_bytes);
the most recent lines that fit are drawn, and the embedded scroller keeps its
scrollbars in sync. There is no stream wrapper — just the byte sink *(the C++
`streambuf`/`otstream` layer is not ported)*.

Because its constructor cannot touch the screen, `Terminal` follows the
deferred-init pattern: build it, insert it into a group, then call
[`init`](../api/tvision_rs/widgets/terminal/struct.Terminal.html#method.init) once
to set up its limits and cursor.

## Edit commands enable themselves

The editor tracks which editing commands make sense for the current state and
enables or disables them on every state change — including when the window gains
or loses focus. The `update_commands` method (`src/widgets/editor.rs`) runs after
every edit and on focus change:

| Command | Enabled when |
| --- | --- |
| `Command::UNDO` | undo history is non-empty (`del_count > 0 || ins_count > 0`) |
| `Command::CUT` | there is a selection |
| `Command::COPY` | there is a selection |
| `Command::PASTE` | no clipboard editor active, or clipboard has a selection |
| `Command::CLEAR` | there is a selection |
| `Command::FIND` | always (while the editor is focused) |
| `Command::REPLACE` | always |
| `Command::SEARCH_AGAIN` | always |
| `Command::SAVE` / `SAVE_AS` | always, for a `FileEditor` |

The enable/disable changes go through the deferred channel
(`ctx.enable_command` / `ctx.disable_command`), so they apply after the current
dispatch. A menu that depends on these commands automatically grays or un-grays
its items at the next pump because the menu's draw reads the current command-set
snapshot.

The **clipboard editor** (`is_clipboard: true`) skips cut/copy/paste updates —
it is a special internal editor used for the clipboard window, which should not
compete with the main editor for those commands.

Source: `src/widgets/editor.rs` (`Editor::update_commands`, `Editor::set_state`).

## Key bindings

The editor resolves keystrokes through the process-global
[`Keymap`](../api/tvision_rs/keymap/struct.Keymap.html) (`src/keymap.rs`). The
keymap maps one- or two-stroke **chords** to `Command` values. Three preset
keymaps ship out of the box:

| Preset | Default? | Style |
| --- | --- | --- |
| `Keymap::word_star()` | **yes** | WordStar / Ctrl-letter diamond + Ctrl-K/Ctrl-Q block prefixes |
| `Keymap::cua()` | no | CUA / "Office" (Ctrl-C copy, Ctrl-V paste, Ctrl-Z undo) |
| `Keymap::emacs()` | no | Emacs / readline (Ctrl-A line-start, Ctrl-E line-end, …) |

The WordStar preset mirrors the C++ editor's `firstKeys` / `quickKeys` /
`blockKeys` tables faithfully:

- **Single-stroke `firstKeys`** — the Ctrl-letter diamond: `Ctrl-S` left,
  `Ctrl-D` right, `Ctrl-E` up, `Ctrl-X` down, etc., plus named keys (`Left`,
  `Right`, `Home`, `End`, `PageUp`, `PageDown`).
- **`Ctrl-Q` prefix** (`quickKeys`) — `Ctrl-Q F` find, `Ctrl-Q A` replace,
  `Ctrl-Q S` line-start, `Ctrl-Q D` line-end, …
- **`Ctrl-K` prefix** (`blockKeys`) — `Ctrl-K B` start-select, `Ctrl-K K`
  copy, `Ctrl-K C` paste, `Ctrl-K Y` cut, `Ctrl-K H` hide-select.

Replace the keymap process-globally with `keymap::set_global`:

```rust
# use tvision_rs as tv;
use tv::keymap::{Keymap, set_global};

// Switch the whole application to CUA bindings.
set_global(Keymap::cua());
```

Or build a custom map from scratch:

```rust
# use tvision_rs as tv;
use tv::keymap::{Keymap, set_global};
use tv::Command;

let mut km = Keymap::word_star();   // start from the WordStar base
km.bind("ctrl+z", Command::UNDO);   // add a CUA undo chord
set_global(km);
```

The two-stroke prefix (`Ctrl-K`, `Ctrl-Q`) works through the `Keymap::resolve`
API: when `resolve(None, stroke)` returns `Resolve::Prefix`, the editor holds the
first stroke as `pending`; the next keystroke is resolved as
`resolve(Some(prefix), stroke)`.

Source: `src/keymap.rs`.

> **Turbo Vision heritage:** the C++ editor maintained a `key_state` machine and
> three hard-coded tables (`firstKeys`, `quickKeys`, `blockKeys`). tvision-rs
> replaces them with a data-driven `Keymap` — the `word_star()` preset is a
> direct transcription of those tables.

## Search and replace

The editor handles `Command::FIND`, `Command::REPLACE`, and
`Command::SEARCH_AGAIN`:

- **`FIND`** — requests a find dialog from the loop via
  `Context::open_find_dialog(editor_id)`. The loop builds the dialog (pre-filled
  with the editor's last search string), runs it, and on OK re-injects a
  `SEARCH_AGAIN` command targeting the editor.
- **`REPLACE`** — same flow with a replace dialog that also has a replacement
  string field and option checkboxes.
- **`SEARCH_AGAIN`** — calls `do_search_replace`, which runs the actual
  search using the stored `find_str`, `replace_str`, and `editor_flags`
  (`EF_DO_REPLACE`, `EF_REPLACE_ALL`, `EF_PROMPT_ON_REPLACE`).

The **prompt-on-replace** path is asynchronous: when the "prompt on replace"
option is checked, each found occurrence triggers a `request_message_box` (Yes /
No / Cancel). The editor caches the answer in `pending_replace_answer` via
`set_modal_answer`, then the loop re-injects `SEARCH_AGAIN`. On the next
dispatch the editor reads the cached answer before searching for the next
occurrence — "Yes" replaces and continues, "No" skips and continues, "Cancel"
aborts the whole loop.

The loop option `EF_REPLACE_ALL` skips the per-occurrence prompt and replaces
every match in a tight loop; `EF_DO_REPLACE` distinguishes replace from find-only.

Source: `src/widgets/editor.rs` (`do_search_replace`, `Command::FIND` /
`REPLACE` arms in `handle_event`, `pending_replace_answer`).

> **Turbo Vision heritage:** `TEditor` used a synchronous nested event loop for
> the search dialogs. tvision-rs routes both the dialog result and the
> prompt-on-replace answer through the deferred/async-modal seam, keeping the
> single event loop intact.

## Editor as a dialog control (Memo)

[`Memo`](../api/tvision_rs/widgets/struct.Memo.html) is a thin wrapper over
`Editor` that turns it into a well-behaved dialog control:

1. **Tab pass-through** — a plain (unmodified) `Tab` keystroke is *not*
   consumed by the editor; it falls through to the dialog's focus navigation.
   `Shift-Tab`, `Ctrl-Tab`, and `Alt-Tab` are forwarded to the editor as normal
   (those bindings have in-editor meaning or are editor-application-specific).
2. **`FieldValue` transfer** — `Memo::value()` returns
   `FieldValue::Text(contents)` and `Memo::set_value(FieldValue::Text(s))`
   loads text into the buffer. This makes `Memo` transparent to the dialog's
   gather/scatter walk.

```rust
# use tvision_rs as tv;
# fn _demo(dialog: &mut tv::Dialog) {
use tv::widgets::Memo;

let memo_id = dialog.insert_child(Box::new(
    Memo::new(
        tv::Rect::new(3, 3, 37, 10),
        None, // h_scroll_bar
        None, // v_scroll_bar
        None, // indicator
        4096, // buffer size in bytes
    )
));
# let _ = memo_id;
# }
```

After `exec_view` returns `Command::OK`, gather the memo's text through the
dialog's child value (index in the gather Vec matches insertion order):

```rust,ignore
use tvision_rs::data::FieldValue;
let values = dialog.gather_data(); // Vec<Option<FieldValue>>
if let Some(Some(FieldValue::Text(text))) = values.first() {
    // text is the memo's current content
    println!("memo: {}", text);
}
```

Source: `src/widgets/editor.rs` (`Memo::handle_event`, `Memo::value`,
`Memo::set_value`), `src/data.rs` (`FieldValue::Text`).

## Where to go next

- [Dialogs & data](dialogs.md) — how a `Memo`'s text flows through
  gather/scatter.
- [Windows & the desktop](windows.md) — placing and tiling editor windows.
- [Deferred effects](../internals/deferred.md) — why a view can request a modal
  save prompt without owning the loop.
