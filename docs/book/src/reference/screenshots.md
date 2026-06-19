# The screenshot tooling

Every colored screen in this guide is the *actual* terminal output of a running
example, captured live and rendered as selectable HTML — not a picture. The
[`hello` screen](../getting-started/first-app.md) you saw earlier is one of
these. This page explains the pipeline and how to add or regenerate a screen.

## The pipeline

A screen is produced entirely by the `xtask` crate, in pure Rust:

```text
run example in a fixed-size tmux pane
  → tmux send-keys            (drive it into the desired state)
  → tmux capture-pane -e -p   (colored ANSI + UTF-8 box drawing)
  → repo-owned ANSI→HTML converter   (themed <pre class="tv-screen">)
  → written to docs/book/src/screens/<name>.html   (committed)
  → embedded into a guide page with {{#include}}
```

Capturing real ANSI beats embedding PNGs: the result is selectable,
copy-pasteable, scalable, tiny, diffable in git, and it supports **interactive**
shots — send keystrokes to open a menu or dialog, then capture the result.

The ANSI→HTML converter is repo-owned (no external `aha` dependency). It resolves
the base SGR colors through [`Color::BIOS_RGB`](../api/tvision_rs/color/enum.Color.html),
so an embedded screenshot uses exactly the palette the running crate uses. It
also handles the bright colors, bold/underline/reverse, the xterm 256-color cube,
and 24-bit truecolor.

## The screen registry

Each documented screen is one entry in the `SCREENS` table in
`xtask/src/screens.rs`. An entry names the example to run, the terminal size, the
keystrokes to drive it, and a settle delay:

```rust,ignore
// Illustrative sketch — not a standalone program.
Screen {
    name: "hello",       // output file stem → src/screens/hello.html
    example: "hello",    // cargo run --example hello
    cols: 80,
    rows: 25,
    keys: &[],           // tmux send-keys arguments, applied in order
    settle_ms: 700,      // wait after launch / between keystrokes for repaint
}
```

Each entry in `keys` is sent with its own `tmux send-keys` call, in order, so you
drive the app the way a user would: `&["F10", "Down", "Enter"]` opens the menu,
moves down, and selects an item before the capture is taken. The capture waits
`settle_ms` after launch and between keystrokes, giving the app time to repaint.

## The blank-capture guard

A capture can come back empty if the app had not finished painting yet (or died
on launch). Writing that blank pane would clobber the committed screenshot, so the
capture step runs a `looks_blank` check: a pane that contains only whitespace and
escape sequences — no glyphs — is rejected as an error. The committed file is left
untouched, and the message suggests a longer `settle_ms`. The detached tmux
session is always killed afterward, even on error, so no sessions leak.

## Adding or regenerating a screen

1. Add (or reuse) a runnable example under `examples/`.
2. Add a `Screen` entry to `SCREENS` in `xtask/src/screens.rs`, with the keys that
   drive it into the state you want to show.
3. Regenerate:

   ```console
   $ cargo xtask screens
   ```

   This pre-builds every referenced example, then captures each into
   `docs/book/src/screens/<name>.html`. (`cargo xtask docs` runs the same step as
   the first stage of the full site build, and skips it with a warning if tmux is
   unavailable — the committed screens stay usable.)
4. Embed it in a guide page:

   ```text
   {{#include ../screens/<name>.html}}
   ```

5. Commit both the new HTML under `docs/book/src/screens/` and your `screens.rs`
   change. The generated files are checked in, so a plain `mdbook` build (or a
   reviewer without tmux) still renders the guide with screenshots.

If a regeneration reports a blank capture, bump that screen's `settle_ms` and run
`cargo xtask screens` again.
