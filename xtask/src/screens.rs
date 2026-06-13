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
    /// CLI arguments passed to the example after `--` (e.g. the `gallery`
    /// example takes a widget name). Empty = no arguments.
    pub args: &'static [&'static str],
    pub cols: u16,
    pub rows: u16,
    /// tmux `send-keys` arguments applied in order (each a single send-keys call).
    pub keys: &'static [&'static str],
    /// Milliseconds to wait after launch / between key groups for repaint.
    pub settle_ms: u64,
}

/// The registry: one entry per documented screen. The `hello` proof screen plus
/// one `gallery` entry per widget (`<name>.html` under `src/screens/`).
pub const SCREENS: &[Screen] = &[
    Screen {
        name: "hello",
        example: "hello",
        args: &[],
        cols: 80,
        rows: 25,
        keys: &[],
        settle_ms: 700,
    },
    Screen {
        name: "button",
        example: "gallery",
        args: &["button"],
        cols: 40,
        rows: 12,
        keys: &[],
        settle_ms: 700,
    },
    Screen {
        name: "menubar",
        example: "gallery",
        args: &["menubar"],
        cols: 56,
        rows: 12,
        // Open the File pull-down so the screenshot shows it expanded.
        keys: &["M-f"],
        settle_ms: 700,
    },
    Screen {
        name: "statusline",
        example: "gallery",
        args: &["statusline"],
        cols: 56,
        rows: 8,
        keys: &[],
        settle_ms: 700,
    },
    Screen {
        name: "checkboxes",
        example: "gallery",
        args: &["checkboxes"],
        cols: 44,
        rows: 12,
        keys: &[],
        settle_ms: 700,
    },
    Screen {
        name: "radiobuttons",
        example: "gallery",
        args: &["radiobuttons"],
        cols: 44,
        rows: 12,
        keys: &[],
        settle_ms: 700,
    },
    Screen {
        name: "inputline",
        example: "gallery",
        args: &["inputline"],
        cols: 50,
        rows: 12,
        keys: &[],
        settle_ms: 700,
    },
    Screen {
        name: "statictext",
        example: "gallery",
        args: &["statictext"],
        cols: 50,
        rows: 15,
        keys: &[],
        settle_ms: 700,
    },
    Screen {
        name: "scrollbar",
        example: "gallery",
        args: &["scrollbar"],
        cols: 50,
        rows: 18,
        keys: &[],
        settle_ms: 700,
    },
    Screen {
        name: "history",
        example: "gallery",
        args: &["history"],
        cols: 56,
        rows: 12,
        keys: &[],
        settle_ms: 700,
    },
    Screen {
        name: "dialog",
        example: "gallery",
        args: &["dialog"],
        cols: 52,
        rows: 18,
        keys: &[],
        settle_ms: 700,
    },
    Screen {
        name: "memo",
        example: "gallery",
        args: &["memo"],
        cols: 56,
        rows: 18,
        keys: &[],
        settle_ms: 700,
    },
    Screen {
        name: "colorpicker",
        example: "gallery",
        args: &["colorpicker"],
        cols: 70,
        rows: 27,
        keys: &[],
        settle_ms: 700,
    },
    Screen {
        name: "messagebox",
        example: "gallery",
        args: &["messagebox"],
        cols: 56,
        rows: 15,
        keys: &[],
        settle_ms: 700,
    },
    Screen {
        name: "window",
        example: "gallery",
        args: &["window"],
        cols: 56,
        rows: 20,
        keys: &[],
        settle_ms: 700,
    },
    Screen {
        name: "editor",
        example: "gallery",
        args: &["editor"],
        cols: 76,
        rows: 25,
        keys: &[],
        settle_ms: 700,
    },
    Screen {
        name: "listbox",
        example: "gallery",
        args: &["listbox"],
        cols: 52,
        rows: 18,
        keys: &[],
        settle_ms: 700,
    },
    Screen {
        name: "terminal",
        example: "gallery",
        args: &["terminal"],
        cols: 64,
        rows: 20,
        keys: &[],
        settle_ms: 700,
    },
    Screen {
        name: "outline",
        example: "gallery",
        args: &["outline"],
        cols: 52,
        rows: 25,
        keys: &[],
        settle_ms: 700,
    },
    Screen {
        name: "filedialog",
        example: "gallery",
        args: &["filedialog"],
        cols: 70,
        rows: 24,
        keys: &[],
        settle_ms: 1000,
    },
    Screen {
        name: "chdirdialog",
        example: "gallery",
        args: &["chdirdialog"],
        cols: 64,
        rows: 24,
        keys: &[],
        settle_ms: 1000,
    },
];

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
    let argstr = if s.args.is_empty() {
        String::new()
    } else {
        format!(" -- {}", s.args.join(" "))
    };
    let run = format!(
        "cargo run --quiet --example {}{} ; tmux wait-for -S done_{}",
        s.example, argstr, s.name
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

    // Pre-build each referenced example once (4-core cap per project policy).
    // Several screens share one example (e.g. every `gallery` widget), so
    // dedupe to avoid redundant freshness checks.
    let mut built: Vec<&str> = Vec::new();
    for s in SCREENS {
        if built.contains(&s.example) {
            continue;
        }
        built.push(s.example);
        let status = Command::new("cargo")
            .args(["build", "--quiet", "--example", s.example])
            .env("CARGO_BUILD_JOBS", "4")
            .status()
            .context("pre-build example")?;
        anyhow::ensure!(status.success(), "example {} failed to build", s.example);
    }

    // A blank/flaky capture must NEVER clobber a committed screenshot, so
    // `capture_one` errors rather than writing it. Across a large batch a single
    // timing flake should not abort the rest — warn, keep the committed file,
    // and continue; report the count at the end.
    let mut written = 0usize;
    let mut skipped: Vec<&str> = Vec::new();
    for s in SCREENS {
        eprintln!("  capturing screen '{}' ({}x{})", s.name, s.cols, s.rows);
        match capture_one(s) {
            Ok(html) => {
                let path = dir.join(format!("{}.html", s.name));
                std::fs::write(&path, html).with_context(|| format!("write {}", path.display()))?;
                written += 1;
            }
            Err(e) => {
                eprintln!("  warning: keeping committed '{}.html' — {e:#}", s.name);
                skipped.push(s.name);
            }
        }
    }
    eprintln!(
        "  wrote {written} screen(s) to {} ({} kept on flaky capture)",
        dir.display(),
        skipped.len()
    );
    if !skipped.is_empty() {
        eprintln!("  flaky (committed file kept): {}", skipped.join(", "));
    }
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
