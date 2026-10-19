//! Build the integrated documentation site:
//!   1. mdBook (library) -> docs/book/book/
//!   2. cargo doc (rustdoc) -> $target/doc/
//!   3. copy rustdoc into docs/book/book/api/
//!   4. internal link check over the assembled tree.

use crate::{linkcheck, paths, screens};
use anyhow::{Context, Result};
use mdbook_driver::MDBook;
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

/// mdBook via the library API.
pub fn build_book() -> Result<()> {
    let book = MDBook::load(paths::book_root()).map_err(|e| anyhow::anyhow!("load book: {e}"))?;
    book.build()
        .map_err(|e| anyhow::anyhow!("build book: {e}"))?;
    Ok(())
}

/// rustdoc for the `tvision-rs` crate, with the shared header injected. (The logo
/// is set via `#![doc(html_logo_url = …)]` crate attributes in Plan 2; only the
/// header — carrying the Guide⇄API toggle — is injected here, using only stable
/// rustdoc flags. The header path must be space-free; the repo path is.)
///
/// rustdoc is built into an xtask-owned, isolated target dir
/// (`paths::rustdoc_target_dir()`) rather than the shared `$CARGO_TARGET_DIR`,
/// so `api/` ends up holding only the `tvision-rs` docs. See `paths::rustdoc_out`.
fn build_rustdoc() -> Result<()> {
    let header = paths::book_root().join("theme").join("rustdoc-header.html");
    anyhow::ensure!(
        !header.to_string_lossy().contains(' '),
        "rustdoc-header path contains a space; RUSTDOCFLAGS would be malformed: {}",
        header.display()
    );
    let flags = format!("--html-in-header {}", header.display());
    let target = paths::rustdoc_target_dir();
    let status = Command::new("cargo")
        .args(["doc", "--no-deps", "--package", "tvision-rs"])
        .arg("--target-dir")
        .arg(&target)
        .env("CARGO_BUILD_JOBS", "4")
        .env("RUSTDOCFLAGS", flags)
        .current_dir(paths::workspace_root())
        .status()
        .context("spawn cargo doc")?;
    anyhow::ensure!(status.success(), "cargo doc failed");
    Ok(())
}

/// Copy the rustdoc HTML tree into `docs/book/book/api`.
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
