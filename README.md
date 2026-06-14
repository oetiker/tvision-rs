<div align="center">

# rstv

**Turbo Vision, reborn in idiomatic Rust.**

A faithful Rust port of [magiblot/tvision](https://github.com/magiblot/tvision) —
the modern C++ incarnation of Borland's **Turbo Vision** — with overlapping
windows, pull-down menus, dialogs, a status line, and the whole DOS-era widget
set, running in any modern terminal.

![rstv demo: the tvdemo example — menus, About dialog, a calculator driven 7×6=42,
a window drag, a truecolor color picker, a splitter grid, and cascaded windows](docs/demo/tvdemo.webp)

[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Guide](https://img.shields.io/badge/docs-guide-success.svg)](https://oetiker.github.io/rstv/)
[![API](https://img.shields.io/badge/docs-API-informational.svg)](https://oetiker.github.io/rstv/api/rstv/)

</div>

## What is it?

If you ever drove a DOS-era dialog with overlapping windows, a top menu bar and a
bottom status line, you already know what an app built with rstv looks and feels
like. The goal is a framework a Turbo Vision veteran recognises on sight — but
that is *native Rust*: no inheritance, no raw pointers, no preprocessor.

- The class hierarchy became a `View` **trait** plus `ViewState` **composition**.
- `TView*` pointers became `ViewId` **handles**.
- The `cmXxx` / `sfXxx` constant families became namespaced consts and **named
  boolean fields**.

None of that changes the *behaviour*: the single event loop, the modal dialogs,
the whole-tree draw model and the widget set all work the way Turbo Vision always
has. The pervasive translation choices are catalogued as numbered deviations
(D1–D15) in the guide.

## Try it

```sh
cargo run --example tvdemo     # the classic Turbo Vision demo (shown above)
cargo run --example hello      # a minimal app: desktop + menu + dialog
cargo run --example tvedit     # a multi-window text editor
cargo run --example gallery -- # list the widget gallery; pass a name to view one
```

## Use it

The crate is conventionally imported under the short alias `tv` (the `T` prefix
becomes the `tv::` namespace — `TButton` → `tv::Button`, `TDialog` → `tv::Dialog`):

```toml
[dependencies]
tv = { package = "rstv", version = "0.1" }
```

```rust
use tv::{Button, Command, Dialog, Rect};

let mut dialog = Dialog::new(Rect::new(0, 0, 40, 11), Some("Sign in".into()));
dialog.insert_child(Box::new(Button::new(
    Rect::new(15, 8, 25, 10),
    "~O~K",
    Command::OK,
    Default::default(),
)));
```

See [`examples/hello.rs`](examples/hello.rs) for a complete, runnable program, and
the [Getting Started guide](https://oetiker.github.io/rstv/getting-started/) for a
walk-through.

## Documentation

- **[Developer Guide](https://oetiker.github.io/rstv/)** — the narrative you read
  to learn: getting started, building apps, how it works, and the widget gallery
  with live screenshots.
- **[API reference](https://oetiker.github.io/rstv/api/rstv/)** — the rustdoc.

Both layers read as one site; every code snippet is pulled from compiled examples
and verified in CI, so the docs never drift.

## What's inside

Windows · dialogs · the desktop · pull-down menus · a status line · buttons,
check boxes, radio buttons, input lines (with picture-mask & regex validators),
list boxes, scrollers and scroll bars, a full text editor, an outline viewer, a
truecolor color picker, a splitter, file/directory dialogs, history lists, and a
themeable palette. Rendering is a vendored ratatui cell-buffer with whole-tree
diffing, behind a `Backend` trait (crossterm by default; a headless backend
drives the snapshot tests).

## Heritage & license

rstv is licensed under the [MIT License](LICENSE). It carries forward the upstream
terms of the projects it ports and vendors — Turbo Vision (public domain,
Borland), magiblot/tvision (MIT), and ratatui (MIT) — recorded in
[`NOTICE`](NOTICE).

> The demo animation is recorded with an owned, dependency-free tool —
> `cargo xtask demo` — that drives the `tvdemo` example in tmux and rasterizes
> each frame to an animated WebP. No external recorder required.
