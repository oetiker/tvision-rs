# Keyboard & key mapping

When a key is pressed, it reaches your views as a key-down event; text widgets
turn that keystroke into an editing *command* through a **keymap**. This page
covers both halves: the key model your `View`s see, and the configurable
`Keymap` that drives the editor and input line.

## The key model

A keystroke is a physical [`Key`](../api/tvision_rs/event/enum.Key.html) plus a
separate [`KeyModifiers`](../api/tvision_rs/event/struct.KeyModifiers.html) channel,
bundled into a [`KeyEvent`](../api/tvision_rs/event/struct.KeyEvent.html). There are
deliberately **no** modifier-combined variants: keys and modifiers are always
separate. `Ctrl+C` is
`Key::Char('c')` with `ctrl` set; `Shift+Tab` is `Key::Tab` with `shift`;
`Alt+F3` is `Key::F(3)` with `alt`. The enum holds only base keys — characters,
function keys, and the navigation/editing keys (`Enter`, `Tab`, arrows, `Home`,
`PageUp`, `Insert`, `Delete`, …). The three logical modifiers `shift`, `ctrl`,
and `alt` collapse the platform's left/right distinctions *(mirroring the C++
`kb*Shift` masks)*.

Two helpers bridge the old DOS conventions into this model:

- [`ctrl_to_arrow`](../api/tvision_rs/event/fn.ctrl_to_arrow.html) maps the WordStar
  Ctrl-letter diamond (`Ctrl+S`/`D`/`E`/`X` and friends) to the equivalent arrow
  and navigation keys, clearing all modifiers on a match and passing everything
  else through unchanged — a faithful port of `ctrlToArrow`.
- [`hot_key`](../api/tvision_rs/event/fn.hot_key.html) extracts the `~`-delimited
  accelerator character from a label (uppercased), so a button or menu item
  knows which `Alt+` chord activates it.

## The keymap

Text input does not hard-wire keys to editing actions. Instead a
[`Keymap`](../api/tvision_rs/keymap/struct.Keymap.html) maps a **chord** to a
[`Command`](../api/tvision_rs/command/struct.Command.html) by name — the same
data-driven shape as a VS Code keybindings file. A chord is one keystroke, or
two for a prefix sequence in the classic `Ctrl-K`/`Ctrl-Q` editor style. You
describe chords as strings: space-separated strokes, each a `+`-joined list of
modifiers ending in a key name.

```rust
# use tvision_rs as tv;
let mut km = tv::keymap::Keymap::new();
km.bind("ctrl+s", tv::Command::CHAR_LEFT)        // single stroke
  .bind("ctrl+k ctrl+c", tv::Command::COPY)      // two-stroke prefix chord
  .bind("shift+insert", tv::Command::PASTE);     // shift kept where it matters
km.unbind("ctrl+s");
```

Keystrokes are **normalized** before lookup so the presets stay small and the
historic behaviours survive: alphabetic characters lowercase and drop `shift`
(`ctrl+q a` equals `ctrl+q A`), and the cursor-pad keys drop `shift` too, since
`Shift+Left` is a *selection* modifier handled inside the widget, not a distinct
binding. Keys where shift carries meaning — `Shift+Insert` for paste — keep it.

Resolving a stroke (optionally combined with a pending prefix) yields a
[`Resolve`](../api/tvision_rs/keymap/enum.Resolve.html): a fully matched `Command`,
a `Prefix` signal meaning "this begins a two-stroke chord, hold the next key", or
`None` so the caller treats the key as insertable text or lets it bubble.

## Presets and the global keymap

Three ready-made keymaps ship as constructors:

| Preset       | Constructor          | Feel                                                  |
| ------------ | -------------------- | ----------------------------------------------------- |
| WordStar     | `Keymap::word_star()`| the classic editor diamond + `Ctrl-K`/`Ctrl-Q` blocks |
| CUA / Office | `Keymap::cua()`      | modern `Ctrl+C`/`X`/`V`/`Z` muscle memory             |
| Emacs        | `Keymap::emacs()`    | readline/Cocoa bindings (`Ctrl+A`/`E`/`K`/`Y`)        |

There is one **process-global** keymap, the default for all text input; it starts
as `word_star()`. Swap it once at startup with
[`set_global`](../api/tvision_rs/keymap/fn.set_global.html), and the editor and every
input field follow:

```rust
# use tvision_rs as tv;
tv::keymap::set_global(tv::keymap::Keymap::cua());
```

Widgets consult it via
[`resolve_global`](../api/tvision_rs/keymap/fn.resolve_global.html); you rarely call
that yourself, but it is how a custom text view would join the same binding
scheme.

## Where to go next

- [Commands & events](commands.md) — what those resolved `Command`s do once a
  view consumes them.
- [Text editing](text-editing.md) — the `Memo`, `Editor` and input-line widgets
  the keymap drives.
- [Events → enum + match](../port/events.md) — the design behind the decomposed
  `Key`/`KeyModifiers` model, for Turbo Vision veterans.
