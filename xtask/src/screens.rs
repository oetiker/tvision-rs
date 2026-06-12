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
        "new-session",
        "-d",
        "-s",
        &session,
        "-x",
        &s.cols.to_string(),
        "-y",
        &s.rows.to_string(),
        "bash",
        "-lc",
        &run,
    ])
    .context("tmux new-session failed")?;

    // Drive + capture in an inner fn so the kill below ALWAYS runs, even when
    // a send-keys/capture-pane step errors out (no orphaned tmux sessions).
    let result = drive_and_capture(s, &session);
    let _ = tmux(&["kill-session", "-t", &session]);
    result
}

/// Drive the running session with the configured keystrokes and capture its
/// colored output as an HTML fragment. The session is killed by the caller.
fn drive_and_capture(s: &Screen, session: &str) -> Result<String> {
    sleep_ms(s.settle_ms);
    for key in s.keys {
        tmux(&["send-keys", "-t", session, key])?;
        sleep_ms(s.settle_ms.max(200));
    }

    let captured =
        tmux(&["capture-pane", "-t", session, "-e", "-p"]).context("capture-pane failed")?;
    let ansi = String::from_utf8_lossy(&captured.stdout).into_owned();

    // A flaky capture (the app had not painted yet, or it died on launch) comes
    // back as a blank pane. Writing that would clobber the committed screenshot,
    // so treat it as an error — the caller keeps the committed file.
    anyhow::ensure!(
        !looks_blank(&ansi),
        "screen '{}' captured blank — the terminal was not painted (try a longer settle_ms)",
        s.name
    );

    Ok(ansi_to_html(&ansi))
}

/// True when a capture has no visible content — only whitespace and SGR escape
/// sequences, no glyphs. Used to reject flaky blank captures before they
/// overwrite a committed screenshot.
fn looks_blank(ansi: &str) -> bool {
    let mut chars = ansi.chars().peekable();
    while let Some(c) = chars.next() {
        if c == '\x1b' {
            // Skip a CSI escape sequence: ESC '[' … <final byte 0x40..=0x7e>.
            if chars.peek() == Some(&'[') {
                chars.next();
                for e in chars.by_ref() {
                    if ('\x40'..='\x7e').contains(&e) {
                        break;
                    }
                }
            }
            continue;
        }
        if !c.is_whitespace() {
            return false;
        }
    }
    true
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
    fn blank_capture_is_detected() {
        // Spaces, newlines, and bare SGR escapes only → blank.
        assert!(looks_blank("   \n   \n"));
        assert!(looks_blank("\x1b[0m\n\x1b[44m   \x1b[0m\n"));
        // Any glyph (e.g. a box-drawing char or text) → not blank.
        assert!(!looks_blank("\x1b[31m┌─┐\x1b[0m"));
        assert!(!looks_blank("   x   "));
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
