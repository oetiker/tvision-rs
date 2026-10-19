<div align="center">

# tvision-rs

[![CI](https://github.com/oetiker/tvision-rs/actions/workflows/ci.yml/badge.svg)](https://github.com/oetiker/tvision-rs/actions/workflows/ci.yml)
[![Release](https://img.shields.io/github/v/release/oetiker/tvision-rs)](https://github.com/oetiker/tvision-rs/releases/latest)
[![License: MIT](https://img.shields.io/badge/license-MIT-blue.svg)](LICENSE)
[![Guide](https://img.shields.io/badge/docs-guide-success.svg)](https://oetiker.github.io/tvision-rs/)
[![API](https://img.shields.io/badge/docs-API-informational.svg)](https://docs.rs/tvision-rs)

**Turbo Vision in idiomatic Rust.**

A faithful Rust port of [magiblot/tvision](https://github.com/magiblot/tvision), the
modern C++ version of Borland's **Turbo Vision**, with overlapping windows,
pull-down menus, dialogs, a status line and the DOS-era widget set, running in
a terminal.

![tvision-rs demo: the tvdemo example with menus, About dialog, a calculator driven 7×6=42,
a window drag, a truecolor color picker, a splitter grid, and cascaded windows](docs/demo/tvdemo.webp)

</div>

## What is it?

An app built with tvision-rs looks and behaves like a DOS-era Turbo Vision
program: overlapping windows, a menu bar at the top and a status line at the
bottom. A Turbo Vision veteran recognises the framework, and the code is native
Rust without inheritance, raw pointers or a preprocessor.

- The class hierarchy became a `View` **trait** plus `ViewState` **composition**.
- `TView*` pointers became `ViewId` **handles**.
- The `cmXxx` / `sfXxx` constant families became namespaced consts and **named
  boolean fields**.

The single event loop, the modal dialogs, the whole-tree draw model and the
widget set behave as in Turbo Vision. The guide lists each translation choice
as a numbered deviation.

## Try it

```sh
cargo run --example tvdemo     # the classic Turbo Vision demo (shown above)
cargo run --example hello      # a minimal app: desktop + menu + windows
cargo run --example tvedit     # a multi-window text editor
cargo run --example gallery -- # list the widget gallery; pass a name to view one
cargo run --example snake      # a frameless custom view: steer a 🐍 in truecolor
```

## Use it

The crate is conventionally imported under the short alias `tv` (the `T` prefix
becomes the `tv::` namespace: `TButton` → `tv::Button`, `TDialog` → `tv::Dialog`):

```toml
[dependencies]
tv = { package = "tvision-rs", version = "0.18.0" }
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
the [Getting Started guide](https://oetiker.github.io/tvision-rs/) for a
walk-through.

## Documentation

- **[Developer Guide](https://oetiker.github.io/tvision-rs/)**: getting started,
  building apps, how it works, and the widget gallery with screenshots.
- **[API reference](https://docs.rs/tvision-rs)**: the rustdoc (also bundled into the
  guide site at `/api/` under each version).
- **[`docs/` folder](docs/)**: the guide's Markdown sources, the C++→Rust porting
  reference, and the cross-cutting design notes, as raw `.md` for reading on
  GitHub or feeding to tools and LLMs.

The guide and the API reference are served as one site. Code snippets in the
guide are pulled from compiled examples that CI builds.

## What's inside

Windows · dialogs · the desktop · pull-down menus · a status line · buttons,
check boxes, radio buttons, input lines (with picture-mask & regex validators),
a masked password input, list boxes (with incremental find/filter), a sortable
multi-column table, scrollers and scroll bars, a full text editor, a terminal
output view, an outline viewer, a truecolor
color picker, a splitter, file/directory dialogs, history lists, and a
themeable palette. Rendering is a vendored ratatui cell-buffer with whole-tree
diffing, behind a `Backend` trait (crossterm by default; a headless backend
drives the snapshot tests).

## How it was built and where it's going

I ported tvision-rs from C++ using [Claude Code](https://claude.com/claude-code),
working class-by-class through magiblot/tvision in dependency order, with every
widget pinned by a snapshot test against the original's behaviour.

On top of the port I am adding modern, native-Rust widgets and options to the
classic set, such as a sortable table, a multi-pane splitter and a truecolor
color picker. The guide's
[Extensions beyond Turbo Vision](https://oetiker.github.io/tvision-rs/dev/port/extensions.html)
page lists them all; more are planned.

[Tobi Oetiker](https://github.com/oetiker)

## Heritage & license

tvision-rs is licensed under the [MIT License](LICENSE). It carries forward the upstream
terms of the projects it ports and vendors, Turbo Vision (public domain,
Borland), magiblot/tvision (MIT) and ratatui (MIT), recorded in
[`NOTICE`](NOTICE).

> The demo animation is recorded with `cargo xtask demo`, which drives the
> `tvdemo` example in tmux and rasterizes each frame to an animated WebP.
