# Developer Documentation — Tooling & Integrated-Site Machine (Plan 1)

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Build the reproducible, pure-`cargo` machine that compiles the rstv documentation into one integrated GitHub Pages site (mdBook guide at root + rustdoc at `/api/`), with colored tmux screenshots and CI deploy — *the words come in Plan 2.*

**Architecture:** A new `xtask/` workspace crate is the single entry point (`cargo xtask docs [--serve]`, `cargo xtask screens`). It drives **mdBook as a library** (`MDBook::load().build()`), shells out to `cargo doc` for the rustdoc layer, assembles both into one tree, runs an **owned internal-link checker** that validates book↔api cross-links, and converts `tmux capture-pane -e` output to themed HTML via an **owned ANSI→HTML converter** that reuses `tvision::Color::BIOS_RGB`. No mise, no Makefile, no global tool installs.

**Tech Stack:** Rust (edition 2024), `mdbook` + `mdbook-mermaid` (library deps), `notify` + `tiny_http` (serve), `anyhow`, `tmux`, GitHub Actions Pages.

**Scope note:** This plan ships the *machine plus a minimal real book scaffold* that builds, screenshots, assembles, link-checks, and deploys end-to-end. Authoring the Part I–V prose, completing `src/theme/` rustdoc, doctests, and per-page screenshots is **Plan 2** (content), which runs on this machine.

**Conventions:**
- Artifacts land in `$CARGO_TARGET_DIR` (`/home/oetiker/scratch/cargo-target`); `export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target` before any cargo command. The xtask honors this env var.
- Cap compiler/test parallelism at 4 cores: prefix heavy cargo calls with `CARGO_BUILD_JOBS=4` and pass `--test-threads=4` where relevant.
- Every commit message ends with the trailer:
  `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`

---

## File structure (created/modified by this plan)

**New — the xtask crate:**
- `xtask/Cargo.toml` — crate manifest; deps: `tvision` (path), `mdbook`, `mdbook-mermaid`, `notify`, `tiny_http`, `anyhow`.
- `xtask/src/main.rs` — arg dispatch (`docs [--serve]`, `screens`, usage). One responsibility: CLI routing.
- `xtask/src/ansi_html.rs` — ANSI(SGR)→HTML converter. Pure function, fully unit-tested.
- `xtask/src/screens.rs` — screen registry + tmux capture orchestration → HTML files.
- `xtask/src/build.rs` — mdBook(library) build + `cargo doc` + site assembly into one tree.
- `xtask/src/linkcheck.rs` — owned internal-link checker over the assembled tree. Unit-tested.
- `xtask/src/serve.rs` — watch + rebuild + static file server.
- `xtask/src/paths.rs` — workspace/target/book path resolution (honors `CARGO_TARGET_DIR`).

**New — repo wiring:**
- `.cargo/config.toml` — `cargo xtask` alias.
- `.github/workflows/docs.yml` — build + deploy to GitHub Pages.

**New — the book scaffold:**
- `docs/book/book.toml` — mdBook config.
- `docs/book/src/SUMMARY.md` — the Part I–V skeleton (stub pages).
- `docs/book/src/**/*.md` — one stub page per SUMMARY entry (placeholder body, real headings).
- `docs/book/src/screens/.gitkeep` — generated screenshots land here.
- `docs/book/theme/tv.css` — shared identity (palette/logo) for the guide.
- `docs/book/theme/rustdoc-header.html` — injected into rustdoc (`--html-in-header`): "← Guide" link + matching palette + the Guide⇄API toggle.
- `docs/book/theme/logo.svg`, `docs/book/theme/favicon.svg` — shared marks.

**Modified:**
- `Cargo.toml` — add `xtask` to `[workspace] members`; add `default-members` so plain `cargo test` skips xtask.
- `examples/hello.rs` — add `// ANCHOR:` comments (comments only; no behavior change) for the Part I include + the proof screenshot.

---

## Task 0: Scaffold the xtask crate and workspace wiring

**Files:**
- Create: `xtask/Cargo.toml`, `xtask/src/main.rs`, `xtask/src/paths.rs`
- Create: `.cargo/config.toml`
- Modify: `Cargo.toml` (workspace members + default-members)

- [ ] **Step 1: Create the xtask manifest**

Create `xtask/Cargo.toml`:

```toml
[package]
name = "xtask"
version = "0.0.0"
edition = "2024"
publish = false

[dependencies]
tvision = { path = ".." }
mdbook = "0.4"
mdbook-mermaid = "0.14"
notify = "6"
tiny_http = "0.12"
anyhow = "1"
```

- [ ] **Step 2: Add xtask to the workspace; keep default builds lean**

Modify `Cargo.toml` (top of file). Change:

```toml
[workspace]
members = ["tvision-macros"]
```

to:

```toml
[workspace]
members = ["tvision-macros", "xtask"]
# Plain `cargo build`/`test` (no --workspace) skips the heavy doc-tooling crate;
# `cargo test --workspace` still includes it.
default-members = [".", "tvision-macros"]
```

- [ ] **Step 3: Add the cargo alias**

Create `.cargo/config.toml`:

```toml
[alias]
xtask = "run --package xtask --"
```

- [ ] **Step 4: Write path resolution (honors CARGO_TARGET_DIR)**

Create `xtask/src/paths.rs`:

```rust
//! Filesystem locations the doc build needs, resolved relative to the
//! workspace root and honoring `CARGO_TARGET_DIR`.

use std::path::{Path, PathBuf};

/// Workspace root = the directory two levels up from this file's crate
/// (`xtask/`), i.e. the repo root. Resolved from `CARGO_MANIFEST_DIR`.
pub fn workspace_root() -> PathBuf {
    let xtask_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    xtask_dir
        .parent()
        .expect("xtask/ has a parent")
        .to_path_buf()
}

/// The mdBook root: `docs/book`.
pub fn book_root() -> PathBuf {
    workspace_root().join("docs").join("book")
}

/// Built book output: `docs/book/book` (mdBook `build-dir` default).
pub fn book_out() -> PathBuf {
    book_root().join("book")
}

/// Cargo target dir: `$CARGO_TARGET_DIR` if set, else `<root>/target`.
pub fn target_dir() -> PathBuf {
    match std::env::var_os("CARGO_TARGET_DIR") {
        Some(v) => PathBuf::from(v),
        None => workspace_root().join("target"),
    }
}

/// rustdoc output: `<target>/doc`.
pub fn rustdoc_out() -> PathBuf {
    target_dir().join("doc")
}

/// Where generated screenshots are written: `docs/book/src/screens`.
pub fn screens_dir() -> PathBuf {
    book_root().join("src").join("screens")
}
```

- [ ] **Step 5: Write the CLI dispatcher (usage only, for now)**

Create `xtask/src/main.rs`:

```rust
//! rstv documentation build tool. Pure-cargo entry point: `cargo xtask <cmd>`.

mod ansi_html;
mod build;
mod linkcheck;
mod paths;
mod screens;
mod serve;

use anyhow::Result;

fn usage() -> ! {
    eprintln!(
        "cargo xtask <command>\n\
         \n\
         commands:\n\
         \x20 docs [--serve]   build the integrated doc site (guide + api); --serve = watch+serve\n\
         \x20 screens          regenerate the tmux screenshots only\n"
    );
    std::process::exit(2)
}

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    match args.first().map(String::as_str) {
        Some("docs") => {
            let serve = args.iter().any(|a| a == "--serve");
            if serve {
                serve::run()
            } else {
                build::docs()
            }
        }
        Some("screens") => screens::regenerate(),
        _ => usage(),
    }
}
```

- [ ] **Step 6: Add empty module stubs so it compiles**

Create `xtask/src/ansi_html.rs`, `xtask/src/screens.rs`, `xtask/src/build.rs`, `xtask/src/linkcheck.rs`, `xtask/src/serve.rs`, each with a single placeholder that the later tasks replace:

`xtask/src/build.rs`:
```rust
//! mdBook build + rustdoc build + site assembly.
use anyhow::Result;
pub fn docs() -> Result<()> {
    anyhow::bail!("build::docs not implemented yet")
}
```

`xtask/src/screens.rs`:
```rust
//! tmux screenshot capture → themed HTML.
use anyhow::Result;
pub fn regenerate() -> Result<()> {
    anyhow::bail!("screens::regenerate not implemented yet")
}
```

`xtask/src/serve.rs`:
```rust
//! Watch + rebuild + static file server for local preview.
use anyhow::Result;
pub fn run() -> Result<()> {
    anyhow::bail!("serve::run not implemented yet")
}
```

`xtask/src/ansi_html.rs`:
```rust
//! ANSI (SGR) → themed HTML converter.
```

`xtask/src/linkcheck.rs`:
```rust
//! Owned internal-link checker over the assembled site tree.
```

- [ ] **Step 7: Verify it builds and prints usage**

Run:
```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
CARGO_BUILD_JOBS=4 cargo build --package xtask
cargo xtask
```
Expected: builds clean; the second command prints the usage block and exits non-zero.

- [ ] **Step 8: Commit**

```bash
git add xtask .cargo/config.toml Cargo.toml
git commit -m "build(xtask): scaffold cargo-xtask doc tool

New xtask workspace crate is the pure-cargo entry point for the docs
machine (cargo xtask docs|screens). Adds the workspace member, a
default-members list so plain cargo test skips it, the cargo alias, and
path resolution honoring CARGO_TARGET_DIR. Subcommands are stubs.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 1: ANSI→HTML converter (TDD core)

Converts the output of `tmux capture-pane -e -p` (text + SGR escapes) into a
self-contained, HTML-escaped `<pre class="tv-screen">…</pre>` fragment, mapping
the 16 base colors through `tvision::Color::BIOS_RGB` so screenshots match the
crate's palette.

**Files:**
- Modify: `xtask/src/ansi_html.rs` (replace stub)

- [ ] **Step 1: Write the failing tests**

Replace `xtask/src/ansi_html.rs` with the test module first (implementation in Step 3):

```rust
//! ANSI (SGR) → themed HTML converter. Consumes `tmux capture-pane -e -p`
//! output and produces a self-contained `<pre class="tv-screen">` fragment.
//! The 16 base colors resolve through `tvision::Color::BIOS_RGB`, so embedded
//! screenshots match the running crate's palette.

#[cfg(test)]
mod tests {
    use super::ansi_to_html;

    fn rgb(i: usize) -> String {
        let (r, g, b) = tvision::Color::BIOS_RGB[i];
        format!("#{r:02x}{g:02x}{b:02x}")
    }

    #[test]
    fn wraps_in_pre_and_escapes_html() {
        let out = ansi_to_html("a <b> & \"c\"");
        assert!(out.starts_with("<pre class=\"tv-screen\">"));
        assert!(out.trim_end().ends_with("</pre>"));
        assert!(out.contains("a &lt;b&gt; &amp; \"c\""));
        assert!(!out.contains("<b>"));
    }

    #[test]
    fn foreground_base_color_maps_to_bios_rgb() {
        // SGR 31 = foreground BIOS index 1 (blue in the TV palette).
        let out = ansi_to_html("\x1b[31mX\x1b[0m");
        assert!(out.contains(&format!("color:{}", rgb(1))), "got: {out}");
        assert!(out.contains(">X<"));
    }

    #[test]
    fn background_and_bright_and_bold() {
        // 42 = bg index 2; 1 = bold; 97 = fg bright index 15.
        let out = ansi_to_html("\x1b[42;1;97mY\x1b[0m");
        assert!(out.contains(&format!("background:{}", rgb(2))));
        assert!(out.contains(&format!("color:{}", rgb(15))));
        assert!(out.contains("font-weight:bold"));
    }

    #[test]
    fn reset_closes_styling() {
        let out = ansi_to_html("\x1b[31mA\x1b[0mB");
        assert!(out.ends_with("B</pre>\n"), "got: {out}");
        // B sits outside any span: reset returned the state to default.
        let tail = &out[out.rfind("</span>").unwrap()..];
        assert_eq!(tail, "</span>B</pre>\n");
    }

    #[test]
    fn truecolor_fg() {
        let out = ansi_to_html("\x1b[38;2;10;20;30mZ\x1b[0m");
        assert!(out.contains("color:#0a141e"), "got: {out}");
    }

    #[test]
    fn indexed_256_uses_bios_for_low_16() {
        let out = ansi_to_html("\x1b[38;5;1mQ\x1b[0m");
        assert!(out.contains(&format!("color:{}", rgb(1))), "got: {out}");
    }

    #[test]
    fn indexed_256_cube_value() {
        // 16 = first cube cell = rgb(0,0,0).
        let out = ansi_to_html("\x1b[38;5;16mC\x1b[0m");
        assert!(out.contains("color:#000000"), "got: {out}");
        // 231 = last cube cell = rgb(255,255,255).
        let out2 = ansi_to_html("\x1b[38;5;231mD\x1b[0m");
        assert!(out2.contains("color:#ffffff"), "got: {out2}");
    }

    #[test]
    fn preserves_box_drawing_utf8() {
        let out = ansi_to_html("┌─┐");
        assert!(out.contains("┌─┐"));
    }

    #[test]
    fn reverse_swaps_fg_bg() {
        // fg=1, bg=2, then reverse → effective fg uses index 2, bg uses index 1.
        let out = ansi_to_html("\x1b[31;42;7mR\x1b[0m");
        assert!(out.contains(&format!("color:{}", rgb(2))), "got: {out}");
        assert!(out.contains(&format!("background:{}", rgb(1))), "got: {out}");
    }
}
```

- [ ] **Step 2: Run the tests to verify they fail**

Run:
```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
CARGO_BUILD_JOBS=4 cargo test --package xtask ansi_html -- --test-threads=4
```
Expected: FAIL — `cannot find function ansi_to_html`.

- [ ] **Step 3: Implement the converter**

Add above the `#[cfg(test)]` module in `xtask/src/ansi_html.rs`:

```rust
use std::fmt::Write as _;

#[derive(Clone, Copy, PartialEq)]
enum Col {
    Default,
    Rgb(u8, u8, u8),
}

#[derive(Clone, Copy, PartialEq)]
struct Sgr {
    fg: Col,
    bg: Col,
    bold: bool,
    underline: bool,
    reverse: bool,
}

impl Sgr {
    fn reset() -> Self {
        Sgr { fg: Col::Default, bg: Col::Default, bold: false, underline: false, reverse: false }
    }
}

fn bios(i: u8) -> Col {
    let (r, g, b) = tvision::Color::BIOS_RGB[(i & 0x0f) as usize];
    Col::Rgb(r, g, b)
}

/// xterm 256-color index → RGB.
fn xterm256(i: u8) -> Col {
    match i {
        0..=15 => bios(i),
        16..=231 => {
            let i = i - 16;
            let steps = [0u8, 95, 135, 175, 215, 255];
            Col::Rgb(steps[(i / 36) as usize], steps[((i / 6) % 6) as usize], steps[(i % 6) as usize])
        }
        232..=255 => {
            let v = 8 + 10 * (i - 232);
            Col::Rgb(v, v, v)
        }
    }
}

fn push_escaped(out: &mut String, ch: char) {
    match ch {
        '&' => out.push_str("&amp;"),
        '<' => out.push_str("&lt;"),
        '>' => out.push_str("&gt;"),
        c => out.push(c),
    }
}

fn write_style(out: &mut String, s: Sgr) {
    let (mut fg, mut bg) = (s.fg, s.bg);
    if s.reverse {
        std::mem::swap(&mut fg, &mut bg);
    }
    out.push_str("<span style=\"");
    if let Col::Rgb(r, g, b) = fg {
        let _ = write!(out, "color:#{r:02x}{g:02x}{b:02x};");
    }
    if let Col::Rgb(r, g, b) = bg {
        let _ = write!(out, "background:#{r:02x}{g:02x}{b:02x};");
    }
    if s.bold {
        out.push_str("font-weight:bold;");
    }
    if s.underline {
        out.push_str("text-decoration:underline;");
    }
    out.push_str("\">");
}

/// Apply one CSI `…m` parameter list to the running SGR state.
fn apply_sgr(state: &mut Sgr, params: &[i64]) {
    let mut it = params.iter().copied().peekable();
    while let Some(p) = it.next() {
        match p {
            0 => *state = Sgr::reset(),
            1 => state.bold = true,
            22 => state.bold = false,
            4 => state.underline = true,
            24 => state.underline = false,
            7 => state.reverse = true,
            27 => state.reverse = false,
            30..=37 => state.fg = bios((p - 30) as u8),
            90..=97 => state.fg = bios((p - 90 + 8) as u8),
            39 => state.fg = Col::Default,
            40..=47 => state.bg = bios((p - 40) as u8),
            100..=107 => state.bg = bios((p - 100 + 8) as u8),
            49 => state.bg = Col::Default,
            38 | 48 => {
                let target_fg = p == 38;
                match it.next() {
                    Some(5) => {
                        if let Some(n) = it.next() {
                            let c = xterm256(n as u8);
                            if target_fg { state.fg = c } else { state.bg = c }
                        }
                    }
                    Some(2) => {
                        let r = it.next().unwrap_or(0) as u8;
                        let g = it.next().unwrap_or(0) as u8;
                        let b = it.next().unwrap_or(0) as u8;
                        let c = Col::Rgb(r, g, b);
                        if target_fg { state.fg = c } else { state.bg = c }
                    }
                    _ => {}
                }
            }
            _ => {}
        }
    }
}

/// Convert ANSI/SGR text (from `tmux capture-pane -e -p`) to an HTML fragment.
pub fn ansi_to_html(input: &str) -> String {
    let mut out = String::from("<pre class=\"tv-screen\">");
    let mut state = Sgr::reset();
    let mut span_open = false;
    let mut chars = input.chars().peekable();

    let close_span = |out: &mut String, open: &mut bool| {
        if *open {
            out.push_str("</span>");
            *open = false;
        }
    };

    while let Some(ch) = chars.next() {
        if ch == '\x1b' {
            // Expect CSI: '[' … final byte. Only `m` (SGR) is meaningful here;
            // any other CSI final byte is consumed and ignored.
            if chars.peek() == Some(&'[') {
                chars.next();
                let mut buf = String::new();
                let mut final_byte = None;
                for c in chars.by_ref() {
                    if c.is_ascii_alphabetic() {
                        final_byte = Some(c);
                        break;
                    }
                    buf.push(c);
                }
                if final_byte == Some('m') {
                    let params: Vec<i64> = if buf.is_empty() {
                        vec![0]
                    } else {
                        buf.split(';')
                            .map(|s| s.parse::<i64>().unwrap_or(0))
                            .collect()
                    };
                    close_span(&mut out, &mut span_open);
                    apply_sgr(&mut state, &params);
                }
            }
            continue;
        }

        // Printable (or newline). Open a span lazily if the state is non-default.
        if !span_open && state != Sgr::reset() {
            write_style(&mut out, state);
            span_open = true;
        }
        push_escaped(&mut out, ch);
    }

    close_span(&mut out, &mut span_open);
    out.push_str("</pre>\n");
    out
}
```

- [ ] **Step 4: Run the tests to verify they pass**

Run:
```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
CARGO_BUILD_JOBS=4 cargo test --package xtask ansi_html -- --test-threads=4
```
Expected: PASS (all 9 tests).

- [ ] **Step 5: Lint and commit**

```bash
CARGO_BUILD_JOBS=4 cargo clippy --package xtask --all-targets -- -D warnings
git add xtask/src/ansi_html.rs
git commit -m "feat(xtask): ANSI->HTML screenshot converter

Converts tmux capture-pane -e output to a self-contained, HTML-escaped
<pre class=tv-screen> fragment. 16 base colors resolve through
tvision::Color::BIOS_RGB so embedded screenshots match the crate palette;
handles 256-color, truecolor, bold/underline/reverse. Unit-tested.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 2: Screen registry + tmux capture

Defines the screenshots the docs need and produces an HTML file per screen by
running an example in a fixed-size tmux pane, sending keystrokes, and capturing
with `-e`.

**Files:**
- Modify: `xtask/src/screens.rs` (replace stub)
- Modify: `xtask/src/paths.rs` (already has `screens_dir`)

- [ ] **Step 1: Write the failing test for the registry/escape contract**

Replace `xtask/src/screens.rs` with the test + types first:

```rust
//! Screenshot capture: run an example in a fixed-size tmux pane, drive it with
//! keystrokes, capture colored output, convert to HTML (see `ansi_html`).

use crate::ansi_html::ansi_to_html;
use crate::paths;
use anyhow::{Context, Result};
use std::process::Command;

/// One documented screen: which example to run, terminal size, keys to send to
/// reach the desired state, and the output file stem (under `src/screens/`).
pub struct Screen {
    pub name: &'static str,
    pub example: &'static str,
    pub cols: u16,
    pub rows: u16,
    /// tmux `send-keys` arguments applied in order (each a single send-keys call).
    pub keys: &'static [&'static str],
    /// Milliseconds to wait after launch / between key groups for repaint.
    pub settle_ms: u64,
}

/// The registry. Plan 2 grows this; Plan 1 ships exactly one proof screen.
pub const SCREENS: &[Screen] = &[Screen {
    name: "hello",
    example: "hello",
    cols: 80,
    rows: 25,
    keys: &[],
    settle_ms: 700,
}];

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn registry_is_nonempty_and_well_formed() {
        assert!(!SCREENS.is_empty());
        for s in SCREENS {
            assert!(!s.name.is_empty());
            assert!(!s.example.is_empty());
            assert!(s.cols >= 20 && s.rows >= 5);
        }
    }

    #[test]
    fn capture_to_html_wraps_capture_output() {
        // The capture→HTML seam is just ansi_to_html; verify the contract here
        // without needing tmux in unit tests.
        let html = ansi_to_html("\x1b[31m┌─┐\x1b[0m");
        assert!(html.contains("tv-screen"));
        assert!(html.contains("┌─┐"));
    }
}
```

- [ ] **Step 2: Run to verify it fails (the impl fns are missing)**

Run:
```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
CARGO_BUILD_JOBS=4 cargo test --package xtask screens -- --test-threads=4
```
Expected: FAIL — `regenerate` / `capture_one` not found (referenced by `main.rs`), or unused-import errors. (You will make it pass in Step 3.)

- [ ] **Step 3: Implement capture + regenerate**

Append to `xtask/src/screens.rs` (before the test module):

```rust
fn tmux(args: &[&str]) -> Result<std::process::Output> {
    let out = Command::new("tmux")
        .args(args)
        .output()
        .context("failed to spawn tmux — is it installed?")?;
    Ok(out)
}

fn sleep_ms(ms: u64) {
    std::thread::sleep(std::time::Duration::from_millis(ms));
}

/// Launch one screen in a detached tmux session, drive it, capture colored
/// output, and return the HTML fragment.
pub fn capture_one(s: &Screen) -> Result<String> {
    let session = format!("rstvdoc_{}", s.name);
    let _ = tmux(&["kill-session", "-t", &session]);

    // The example binary path: build it first so launch is instant & stable.
    let run = format!(
        "cargo run --quiet --example {} ; tmux wait-for -S done_{}",
        s.example, s.name
    );
    tmux(&[
        "new-session", "-d", "-s", &session,
        "-x", &s.cols.to_string(), "-y", &s.rows.to_string(),
        "bash", "-lc", &run,
    ])
    .context("tmux new-session failed")?;

    sleep_ms(s.settle_ms);
    for key in s.keys {
        tmux(&["send-keys", "-t", &session, key])?;
        sleep_ms(s.settle_ms.max(200));
    }

    let captured = tmux(&["capture-pane", "-t", &session, "-e", "-p"])
        .context("capture-pane failed")?;
    let ansi = String::from_utf8_lossy(&captured.stdout).into_owned();

    let _ = tmux(&["kill-session", "-t", &session]);
    Ok(ansi_to_html(&ansi))
}

/// Regenerate every screen's HTML under `docs/book/src/screens/`.
pub fn regenerate() -> Result<()> {
    let dir = paths::screens_dir();
    std::fs::create_dir_all(&dir).context("create screens dir")?;

    // Pre-build all referenced examples once (4-core cap per project policy).
    for s in SCREENS {
        let status = Command::new("cargo")
            .args(["build", "--quiet", "--example", s.example])
            .env("CARGO_BUILD_JOBS", "4")
            .status()
            .context("pre-build example")?;
        anyhow::ensure!(status.success(), "example {} failed to build", s.example);
    }

    for s in SCREENS {
        eprintln!("  capturing screen '{}' ({}x{})", s.name, s.cols, s.rows);
        let html = capture_one(s)?;
        let path = dir.join(format!("{}.html", s.name));
        std::fs::write(&path, html).with_context(|| format!("write {}", path.display()))?;
    }
    eprintln!("  wrote {} screen(s) to {}", SCREENS.len(), dir.display());
    Ok(())
}
```

- [ ] **Step 4: Run unit tests; then prove capture end-to-end**

Run:
```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
CARGO_BUILD_JOBS=4 cargo test --package xtask screens -- --test-threads=4
cargo xtask screens
```
Expected: tests PASS; `cargo xtask screens` writes `docs/book/src/screens/hello.html`.
Verify it contains color + box-drawing:
```bash
grep -o 'tv-screen' docs/book/src/screens/hello.html | head -1
grep -c 'background:#' docs/book/src/screens/hello.html
```
Expected: prints `tv-screen`; the count is > 0 (the desktop paints colored cells).

- [ ] **Step 5: Commit (including the generated proof screen)**

```bash
git add xtask/src/screens.rs docs/book/src/screens/
git commit -m "feat(xtask): tmux screenshot capture pipeline

Screen registry + capture_one(): runs an example in a fixed-size tmux pane,
drives it with send-keys, captures with capture-pane -e, and converts to a
themed HTML fragment. cargo xtask screens regenerates all screens. Ships one
proof screen (hello).

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 3: mdBook scaffold (the Part I–V skeleton)

A minimal but real book so the build has content. Stub pages carry the correct
headings (Plan 2 fills the prose).

**Files:**
- Create: `docs/book/book.toml`, `docs/book/src/SUMMARY.md`, stub `*.md` pages, `docs/book/theme/` assets.

- [ ] **Step 1: Write `book.toml`**

Create `docs/book/book.toml`:

```toml
[book]
title = "tvision — Developer Guide"
description = "Building TUI applications with the tvision crate (rstv)."
authors = ["Tobias Oetiker"]
language = "en"
src = "src"

[build]
build-dir = "book"

# NOTE: mermaid is registered programmatically by xtask (library preprocessor),
# so it is intentionally NOT listed as a [preprocessor.mermaid] subprocess here.
# The mermaid runtime JS is added via additional-js below.

[output.html]
additional-css = ["theme/tv.css"]
additional-js = ["theme/mermaid.min.js", "theme/mermaid-init.js"]
default-theme = "navy"
preferred-dark-theme = "navy"
git-repository-url = "https://github.com/oetiker/rstv"
edit-url-template = "https://github.com/oetiker/rstv/edit/main/docs/book/{path}"
site-url = "/rstv/"

[output.html.fold]
enable = true
```

- [ ] **Step 2: Write `SUMMARY.md` (the full Part I–V skeleton)**

Create `docs/book/src/SUMMARY.md`:

```markdown
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
```

- [ ] **Step 3: Generate every stub page**

Run this one-shot generator (creates a heading-only page for each SUMMARY link that doesn't exist yet):

```bash
cd docs/book/src
mkdir -p getting-started port apps internals reference
# README (intro landing)
[ -f README.md ] || printf '# tvision Developer Guide\n\n_Stub. Content lands in Plan 2._\n' > README.md
# every other page from SUMMARY paths
for p in \
  getting-started/installation getting-started/first-app getting-started/skeleton \
  port/faithful port/inheritance port/handles port/events port/flags port/constants \
  port/theme port/draw port/modal port/deferred port/dropped \
  apps/windows apps/dialogs apps/controls apps/menus apps/commands apps/keyboard apps/theming apps/text-editing \
  internals/view-tree internals/event-loop internals/deferred internals/brokering internals/drawing internals/custom-view \
  reference/api reference/symbol-map reference/deviations reference/screenshots ; do
  title=$(basename "$p" | tr '-' ' ')
  [ -f "$p.md" ] || printf '# %s\n\n_Stub. Content lands in Plan 2._\n' "$title" > "$p.md"
done
cd -
```
Expected: 32 stub pages created under `docs/book/src/`.

- [ ] **Step 4: Add the shared theme assets**

Create `docs/book/theme/tv.css`:

```css
/* Shared visual identity for the guide (and, via rustdoc-header.html, the API). */
:root { --tv-blue: #0000aa; --tv-cyan: #00aaaa; --tv-yellow: #aaaa00; }

/* Screenshot frames produced by the ANSI->HTML converter. */
pre.tv-screen {
  display: inline-block;
  background: #0000aa;            /* matches Color::BIOS_RGB[1] */
  color: #aaaaaa;                 /* matches Color::BIOS_RGB[7] */
  padding: 0.5rem 0.75rem;
  border-radius: 4px;
  line-height: 1.15;
  font-variant-ligatures: none;
  border: 1px solid #000;
}

/* The Guide <-> API toggle injected into both surfaces. */
.tv-doc-switch { display: inline-flex; gap: 0.5rem; font-weight: 600; }
.tv-doc-switch a { text-decoration: none; }
```

Create `docs/book/theme/logo.svg` and `docs/book/theme/favicon.svg` — a simple placeholder mark (square with "tv"):

```svg
<svg xmlns="http://www.w3.org/2000/svg" width="64" height="64" viewBox="0 0 64 64">
  <rect width="64" height="64" rx="8" fill="#0000aa"/>
  <text x="32" y="42" font-family="monospace" font-size="28" fill="#aaaaaa" text-anchor="middle">tv</text>
</svg>
```

Create placeholder mermaid assets so `additional-js` resolves even before Task 4 vendors the real ones:

```bash
: > docs/book/theme/mermaid.min.js
printf '// mermaid init placeholder; real asset vendored by xtask Task 4\n' > docs/book/theme/mermaid-init.js
```

- [ ] **Step 5: Commit**

```bash
git add docs/book/book.toml docs/book/src docs/book/theme
git commit -m "docs(book): scaffold mdBook (Part I-V skeleton + theme)

book.toml, SUMMARY.md with the full five-part outline, 32 heading-only stub
pages (prose lands in Plan 2), and the shared identity (tv.css, logo,
favicon, mermaid asset placeholders).

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 4: Build the book (library), the rustdoc, and assemble one tree

**Files:**
- Modify: `xtask/src/build.rs` (replace stub)
- Create: `docs/book/theme/rustdoc-header.html` (injected into rustdoc)

- [ ] **Step 1: Write the rustdoc header (shared identity + Guide⇄API toggle)**

Create `docs/book/theme/rustdoc-header.html`:

```html
<style>
  .tv-doc-switch { position: fixed; top: 8px; right: 12px; z-index: 9999;
    font-family: sans-serif; font-size: 13px; display: inline-flex; gap: .5rem; }
  .tv-doc-switch a { text-decoration: none; padding: 2px 8px; border-radius: 4px;
    background: #0000aa; color: #fff; }
</style>
<div class="tv-doc-switch">
  <a href="/rstv/">Guide</a>
  <a href="/rstv/api/tvision/index.html">API</a>
</div>
```

- [ ] **Step 2: Implement build (book + rustdoc + assemble)**

Replace `xtask/src/build.rs`:

```rust
//! Build the integrated documentation site:
//!   1. mdBook (library) -> docs/book/book/
//!   2. cargo doc (rustdoc) -> $target/doc/
//!   3. copy rustdoc into docs/book/book/api/
//!   4. internal link check over the assembled tree.

use crate::{linkcheck, paths, screens};
use anyhow::{Context, Result};
use mdbook::MDBook;
use std::path::Path;
use std::process::Command;

/// Full `cargo xtask docs` pipeline.
pub fn docs() -> Result<()> {
    // Screens first so the book embeds fresh captures. (Skipped silently if tmux
    // is unavailable — committed screens remain usable.)
    if let Err(e) = screens::regenerate() {
        eprintln!("warning: screenshot regeneration skipped: {e:#}");
    }

    build_book().context("mdBook build")?;
    build_rustdoc().context("rustdoc build")?;
    assemble_api().context("assemble api/ into book")?;

    let root = paths::book_out();
    let broken = linkcheck::check_tree(&root).context("link check")?;
    if !broken.is_empty() {
        for b in &broken {
            eprintln!("  broken link: {b}");
        }
        anyhow::bail!("{} broken internal link(s)", broken.len());
    }

    eprintln!("OK: integrated site at {}", root.display());
    Ok(())
}

/// mdBook via the library API, with the mermaid preprocessor registered in-process.
pub fn build_book() -> Result<()> {
    let mut book =
        MDBook::load(paths::book_root()).map_err(|e| anyhow::anyhow!("load book: {e}"))?;
    book.with_preprocessor(mdbook_mermaid::Mermaid);
    book.build().map_err(|e| anyhow::anyhow!("build book: {e}"))?;
    Ok(())
}

/// rustdoc for the `tvision` crate, with the shared header injected. (The logo
/// is set via `#![doc(html_logo_url = …)]` crate attributes in Plan 2; only the
/// header — carrying the Guide⇄API toggle — is injected here, using only stable
/// rustdoc flags. The header path must be space-free; the repo path is.)
fn build_rustdoc() -> Result<()> {
    let header = paths::book_root().join("theme").join("rustdoc-header.html");
    let flags = format!("--html-in-header {}", header.display());
    let status = Command::new("cargo")
        .args(["doc", "--no-deps", "--package", "tvision"])
        .env("CARGO_BUILD_JOBS", "4")
        .env("RUSTDOCFLAGS", flags)
        .current_dir(paths::workspace_root())
        .status()
        .context("spawn cargo doc")?;
    anyhow::ensure!(status.success(), "cargo doc failed");
    Ok(())
}

/// Copy `$target/doc` into `docs/book/book/api`.
fn assemble_api() -> Result<()> {
    let src = paths::rustdoc_out();
    let dst = paths::book_out().join("api");
    anyhow::ensure!(src.exists(), "rustdoc output missing at {}", src.display());
    if dst.exists() {
        std::fs::remove_dir_all(&dst).ok();
    }
    copy_dir(&src, &dst).with_context(|| format!("copy {} -> {}", src.display(), dst.display()))?;
    Ok(())
}

fn copy_dir(src: &Path, dst: &Path) -> Result<()> {
    std::fs::create_dir_all(dst)?;
    for entry in std::fs::read_dir(src)? {
        let entry = entry?;
        let from = entry.path();
        let to = dst.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_dir(&from, &to)?;
        } else {
            std::fs::copy(&from, &to)?;
        }
    }
    Ok(())
}
```

- [ ] **Step 3: Build the site**

Run:
```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo xtask docs
```
Expected: builds the book, builds rustdoc, copies it to `docs/book/book/api/`, link-check passes; prints `OK: integrated site at …`.
Verify:
```bash
test -f docs/book/book/index.html && echo "guide ok"
test -f docs/book/book/api/tvision/index.html && echo "api ok"
```
Expected: both print "ok".

- [ ] **Step 4: Ignore build output in git**

Append to `.gitignore`:
```bash
printf '\n# mdBook build output (regenerated by `cargo xtask docs`)\ndocs/book/book/\n' >> .gitignore
```

- [ ] **Step 5: Commit**

```bash
git add xtask/src/build.rs docs/book/theme/rustdoc-header.html .gitignore
git commit -m "feat(xtask): build + assemble integrated doc site

Drives mdBook via the library API (mermaid preprocessor registered
in-process), builds rustdoc with a shared injected header (Guide<->API
toggle), and copies the rustdoc into book/api/ so the guide and the API
reference deploy as one tree. Link check gates the build.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 5: Owned internal-link checker (TDD)

Validates every internal `href`/`src` in the assembled tree — crucially the
book→`api/` cross-links a book-only checker can't see.

**Files:**
- Modify: `xtask/src/linkcheck.rs` (replace stub)

- [ ] **Step 1: Write the failing tests**

Replace `xtask/src/linkcheck.rs`:

```rust
//! Internal-link checker over the assembled site tree (book + api/). Verifies
//! that relative href/src targets resolve to files that exist. External links
//! (http(s), mailto, protocol-relative) and pure fragments are skipped.

use anyhow::Result;
use std::path::{Path, PathBuf};

/// Returns a list of `"<file>: <link>"` strings for every broken internal link.
pub fn check_tree(root: &Path) -> Result<Vec<String>> {
    let mut broken = Vec::new();
    let mut html_files = Vec::new();
    collect_html(root, &mut html_files)?;
    for file in &html_files {
        let body = std::fs::read_to_string(file).unwrap_or_default();
        for link in extract_links(&body) {
            if is_external(&link) {
                continue;
            }
            if !resolves(file, &link) {
                let rel = file.strip_prefix(root).unwrap_or(file);
                broken.push(format!("{}: {}", rel.display(), link));
            }
        }
    }
    broken.sort();
    Ok(broken)
}

fn is_external(link: &str) -> bool {
    link.starts_with("http://")
        || link.starts_with("https://")
        || link.starts_with("mailto:")
        || link.starts_with("data:")
        || link.starts_with("//")
        || link.starts_with('#')
}

/// Resolve `link` relative to the directory of `from` and test existence.
fn resolves(from: &Path, link: &str) -> bool {
    let path_part = link.split(['#', '?']).next().unwrap_or("");
    if path_part.is_empty() {
        return true; // pure fragment/query against self
    }
    let base = from.parent().unwrap_or_else(|| Path::new(""));
    let mut target: PathBuf = if let Some(stripped) = path_part.strip_prefix('/') {
        // Site-absolute "/rstv/..." — map onto the assembled root is out of scope
        // for local checking; treat as external/unknown and skip.
        let _ = stripped;
        return true;
    } else {
        base.join(path_part)
    };
    if path_part.ends_with('/') {
        target = target.join("index.html");
    }
    target.exists()
}

fn extract_links(html: &str) -> Vec<String> {
    let mut out = Vec::new();
    for attr in ["href=\"", "src=\""] {
        let mut rest = html;
        while let Some(i) = rest.find(attr) {
            rest = &rest[i + attr.len()..];
            if let Some(end) = rest.find('"') {
                out.push(rest[..end].to_string());
                rest = &rest[end + 1..];
            } else {
                break;
            }
        }
    }
    out
}

fn collect_html(dir: &Path, out: &mut Vec<PathBuf>) -> Result<()> {
    for entry in std::fs::read_dir(dir)? {
        let entry = entry?;
        let p = entry.path();
        if entry.file_type()?.is_dir() {
            collect_html(&p, out)?;
        } else if p.extension().map(|e| e == "html").unwrap_or(false) {
            out.push(p);
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn write(p: &Path, body: &str) {
        std::fs::create_dir_all(p.parent().unwrap()).unwrap();
        std::fs::write(p, body).unwrap();
    }

    #[test]
    fn flags_missing_and_passes_present_and_skips_external() {
        let tmp = std::env::temp_dir().join(format!("rstv_lc_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        write(&tmp.join("a.html"),
            r##"<a href="b.html">ok</a>
               <a href="missing.html">bad</a>
               <a href="api/x.html">cross</a>
               <a href="https://example.com">ext</a>
               <a href="#frag">frag</a>"##);
        write(&tmp.join("b.html"), "<p>b</p>");
        write(&tmp.join("api/x.html"), "<p>x</p>");

        let broken = check_tree(&tmp).unwrap();
        assert_eq!(broken.len(), 1, "broken: {broken:?}");
        assert!(broken[0].contains("missing.html"));
        std::fs::remove_dir_all(&tmp).ok();
    }

    #[test]
    fn directory_link_resolves_to_index() {
        let tmp = std::env::temp_dir().join(format!("rstv_lc2_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        write(&tmp.join("a.html"), r#"<a href="sub/">dir</a>"#);
        write(&tmp.join("sub/index.html"), "<p>i</p>");
        let broken = check_tree(&tmp).unwrap();
        assert!(broken.is_empty(), "broken: {broken:?}");
        std::fs::remove_dir_all(&tmp).ok();
    }
}
```

- [ ] **Step 2: Run to verify tests fail then pass**

Run:
```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
CARGO_BUILD_JOBS=4 cargo test --package xtask linkcheck -- --test-threads=4
```
Expected: with the implementation in place, PASS (2 tests). (If you wrote tests first against an empty module, the first run FAILs to compile — that is the intended red.)

- [ ] **Step 3: Re-run the full site build to confirm the checker is wired**

Run:
```bash
cargo xtask docs
```
Expected: `OK: integrated site at …` (the real tree passes; book→api links resolve).

- [ ] **Step 4: Lint and commit**

```bash
CARGO_BUILD_JOBS=4 cargo clippy --package xtask --all-targets -- -D warnings
git add xtask/src/linkcheck.rs
git commit -m "feat(xtask): owned internal-link checker

Walks the assembled tree and verifies every relative href/src resolves,
including the book->api/ cross-links a book-only checker can't see. Skips
external/fragment links; directory links resolve to index.html. Unit-tested
and wired into cargo xtask docs as a build gate.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 6: `--serve` (watch + rebuild + static server)

**Files:**
- Modify: `xtask/src/serve.rs` (replace stub)

- [ ] **Step 1: Implement serve**

Replace `xtask/src/serve.rs`:

```rust
//! `cargo xtask docs --serve`: build once, serve the assembled tree, and
//! rebuild the book on source changes. Minimal by design.

use crate::{build, paths};
use anyhow::{Context, Result};
use notify::{RecursiveMode, Watcher};
use std::path::Path;
use std::sync::mpsc::channel;

pub fn run() -> Result<()> {
    build::docs().context("initial build")?;

    let root = paths::book_out();
    let addr = "127.0.0.1:3000";
    let server = tiny_http::Server::http(addr)
        .map_err(|e| anyhow::anyhow!("bind {addr}: {e}"))?;
    eprintln!("serving {} at http://{addr}/", root.display());

    // Watch sources; rebuild the book (not rustdoc) on change for fast loops.
    let (tx, rx) = channel();
    let mut watcher = notify::recommended_watcher(move |res| {
        let _ = tx.send(res);
    })?;
    watcher.watch(&paths::book_root().join("src"), RecursiveMode::Recursive)?;
    watcher.watch(&paths::book_root().join("theme"), RecursiveMode::Recursive)?;

    std::thread::spawn(move || {
        for ev in rx {
            if ev.is_ok() {
                eprintln!("change detected — rebuilding book…");
                if let Err(e) = build::build_book() {
                    eprintln!("rebuild error: {e:#}");
                }
            }
        }
    });

    for request in server.incoming_requests() {
        serve_one(&root, request);
    }
    Ok(())
}

fn serve_one(root: &Path, request: tiny_http::Request) {
    let url = request.url().split('?').next().unwrap_or("/");
    let rel = url.trim_start_matches('/');
    let mut path = root.join(rel);
    if path.is_dir() || rel.is_empty() {
        path = path.join("index.html");
    }
    match std::fs::read(&path) {
        Ok(bytes) => {
            let mime = match path.extension().and_then(|e| e.to_str()) {
                Some("html") => "text/html; charset=utf-8",
                Some("css") => "text/css",
                Some("js") => "application/javascript",
                Some("svg") => "image/svg+xml",
                _ => "application/octet-stream",
            };
            let header =
                tiny_http::Header::from_bytes(&b"Content-Type"[..], mime.as_bytes()).unwrap();
            let _ = request.respond(tiny_http::Response::from_data(bytes).with_header(header));
        }
        Err(_) => {
            let _ = request.respond(tiny_http::Response::from_string("404").with_status_code(404));
        }
    }
}
```

- [ ] **Step 2: Smoke-test serve**

Run (in one terminal):
```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo xtask docs --serve
```
Then in another:
```bash
curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:3000/
curl -s -o /dev/null -w "%{http_code}\n" http://127.0.0.1:3000/api/tvision/index.html
```
Expected: both print `200`. Stop the server with Ctrl-C.

- [ ] **Step 3: Lint and commit**

```bash
CARGO_BUILD_JOBS=4 cargo clippy --package xtask --all-targets -- -D warnings
git add xtask/src/serve.rs
git commit -m "feat(xtask): docs --serve (watch + rebuild + static server)

Builds the integrated tree, serves it at 127.0.0.1:3000, and rebuilds the
book on src/theme changes for a fast local loop.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 7: Vendor mermaid assets via xtask (no global install)

Replaces the placeholder mermaid JS with the real runtime, sourced from the
`mdbook-mermaid` crate's bundled assets so there is still no global install.

**Files:**
- Modify: `xtask/src/build.rs` (add asset-sync step)

- [ ] **Step 1: Add an asset-sync that writes the bundled mermaid files**

In `xtask/src/build.rs`, add this function and call it at the top of `build_book()`:

```rust
/// Ensure the mermaid runtime JS exists in the book theme. `mdbook-mermaid`
/// bundles these; we copy them in so a plain mdBook build (and the deployed
/// site) has the runtime without any global `mdbook-mermaid install`.
fn sync_mermaid_assets() -> Result<()> {
    let theme = paths::book_root().join("theme");
    std::fs::create_dir_all(&theme)?;
    // mdbook-mermaid exposes its bundled assets as crate constants.
    std::fs::write(theme.join("mermaid.min.js"), mdbook_mermaid::MERMAID_JS)?;
    std::fs::write(theme.join("mermaid-init.js"), mdbook_mermaid::MERMAID_INIT_JS)?;
    Ok(())
}
```

And in `build_book()`, before `MDBook::load`:

```rust
    sync_mermaid_assets().context("sync mermaid assets")?;
```

> **If the constant names differ** in the pinned `mdbook-mermaid` version, run
> `cargo doc -p mdbook-mermaid` and use the exported asset constants (the crate
> ships `mermaid.min.js` + `mermaid-init.js` as `pub const` byte/str data used by
> its `install` command). Do not shell out to a global binary.

- [ ] **Step 2: Rebuild and verify the real assets landed**

Run:
```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
cargo xtask docs
wc -c docs/book/theme/mermaid.min.js
```
Expected: build OK; `mermaid.min.js` is now large (tens of KB), not the empty placeholder.

- [ ] **Step 3: Commit (assets are git-ignored output of the theme; commit the code only)**

First make sure the vendored JS is tracked (it is part of the deployable theme, not build output):
```bash
git add xtask/src/build.rs docs/book/theme/mermaid.min.js docs/book/theme/mermaid-init.js
git commit -m "feat(xtask): vendor mermaid runtime from crate assets

build_book syncs mdbook-mermaid's bundled JS into the book theme, so mermaid
diagrams render with no global mdbook-mermaid install.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 8: Anchor the hello example (Part I include source)

Adds `// ANCHOR:` comments so the guide can `{{#rustdoc_include}}` real,
compiled code. Comments only — no behavior change.

**Files:**
- Modify: `examples/hello.rs`

- [ ] **Step 1: Add anchors around the app entry point**

In `examples/hello.rs`, wrap the `main` function (and the smallest illustrative
setup it calls) with anchor comments. Example (match the real function bodies in
the file; add only the comment lines):

```rust
// ANCHOR: main
fn main() -> io::Result<()> {
    // …existing body unchanged…
}
// ANCHOR_END: main
```

If the program-construction logic lives in a helper (e.g. `fn build_program()`),
also bracket that with `// ANCHOR: setup` / `// ANCHOR_END: setup`.

- [ ] **Step 2: Verify the example still builds and runs unchanged**

Run:
```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
CARGO_BUILD_JOBS=4 cargo build --example hello
```
Expected: builds clean (anchors are comments).

- [ ] **Step 3: Verify mdBook can include the anchor**

Add a temporary include to `docs/book/src/getting-started/first-app.md`:
```markdown
```rust,no_run
{{#rustdoc_include ../../../../examples/hello.rs:main}}
```
```
Then:
```bash
cargo xtask docs
grep -rl "fn main" docs/book/book/getting-started/first-app.html
```
Expected: build OK and the rendered page contains the included `main`. (Plan 2
replaces this temporary include with the real lesson.)

- [ ] **Step 4: Commit**

```bash
git add examples/hello.rs docs/book/src/getting-started/first-app.md
git commit -m "docs(book): anchor hello example for guide includes

Adds // ANCHOR comments to examples/hello.rs (comments only) so guide pages
can {{#rustdoc_include}} compiled source, and wires a first include on the
first-app page to prove the seam.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Task 9: CI — build + deploy to GitHub Pages

**Files:**
- Create: `.github/workflows/docs.yml`

- [ ] **Step 1: Write the workflow**

Create `.github/workflows/docs.yml`:

```yaml
name: Docs

on:
  push:
    branches: [main]
  workflow_dispatch:

permissions:
  contents: read
  pages: write
  id-token: write

concurrency:
  group: pages
  cancel-in-progress: true

jobs:
  build:
    runs-on: ubuntu-latest
    steps:
      - uses: actions/checkout@v4
      - uses: dtolnay/rust-toolchain@stable
      - uses: Swatinem/rust-cache@v2
      - name: Install tmux (for screenshots)
        run: sudo apt-get update && sudo apt-get install -y tmux
      - name: Build integrated site
        run: cargo xtask docs
      - uses: actions/upload-pages-artifact@v3
        with:
          path: docs/book/book

  deploy:
    needs: build
    runs-on: ubuntu-latest
    environment:
      name: github-pages
      url: ${{ steps.deployment.outputs.page_url }}
    steps:
      - id: deployment
        uses: actions/deploy-pages@v4
```

> **Note:** the workflow runs `cargo xtask docs` with the default
> `CARGO_TARGET_DIR` (no scratch override in CI), so `paths::rustdoc_out()`
> resolves to `<workspace>/target/doc` — exactly what the path helper computes
> when the env var is unset.

- [ ] **Step 2: Validate the workflow file locally**

Run:
```bash
python3 -c "import yaml,sys; yaml.safe_load(open('.github/workflows/docs.yml')); print('yaml ok')"
```
Expected: prints `yaml ok`.

- [ ] **Step 3: Commit**

```bash
git add .github/workflows/docs.yml
git commit -m "ci: build + deploy integrated docs to GitHub Pages

Runs cargo xtask docs (installs tmux for screenshots) and publishes
docs/book/book to Pages on push to main.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

- [ ] **Step 4: (Manual, by repo owner) enable Pages**

In GitHub repo settings → Pages → Source = "GitHub Actions". (Cannot be done
from the CLI plan; note it for the owner.)

---

## Final verification

- [ ] **Step 1: Clean full build from scratch**

```bash
export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target
rm -rf docs/book/book
cargo xtask docs
```
Expected: `OK: integrated site at …`, no broken links.

- [ ] **Step 2: Workspace still green (xtask included)**

```bash
CARGO_BUILD_JOBS=4 cargo test --workspace -- --test-threads=4
CARGO_BUILD_JOBS=4 cargo clippy --workspace --all-targets -- -D warnings
cargo fmt --all --check
```
Expected: all pass.

- [ ] **Step 3: Plain build skips xtask (default-members)**

```bash
CARGO_BUILD_JOBS=4 cargo build
```
Expected: builds `tvision` + `tvision-macros` only (no mdbook deps compiled).

---

## What Plan 2 (content) covers — not in this plan

- Authoring the prose for all 32 pages (Parts I–V) per the spec outline.
- Completing `src/theme/` rustdoc to parity + adding `#![doc(html_logo_url = …,
  html_favicon_url = …)]` crate attributes.
- Promoting reference examples to doctests; adding outbound guide links in module
  `//!` docs.
- Growing `screens::SCREENS` to cover each control/guide page (with `keys` to
  drive interactive states).
- Replacing the temporary `first-app.md` include with the real lesson.
- `mdbook test` + `cargo test --doc` as CI gates once content exists.
```
