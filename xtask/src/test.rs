//! Run the guide's doctests.
//!
//! Compiles every non-`ignore` ```rust block in the mdBook guide
//! (`docs/book/`) as a doctest against the freshly-built `rstv` rlib.
//!
//! We invoke `rustdoc --test` per chapter source file ourselves rather than
//! going through mdBook's `MDBook::test`, because that API only forwards `-L`
//! library-search paths and has no way to pass `--extern rstv=<rlib>`. With
//! only `-L`, a doctest's `use rstv::…;` fails with "no external crate
//! rstv" (the crate is never put in the extern prelude), and an `extern
//! crate` form instead trips over "multiple candidates" whenever the shared
//! target dir holds more than one `librstv-*.rlib`. Passing the exact rlib
//! via `--extern` sidesteps both. `rustdoc --test` on the raw chapter markdown
//! still extracts the blocks, honours `ignore`, and processes hidden `#` lines —
//! mdBook preprocessor directives (`{{#rustdoc_include}}`, ```mermaid```) live in
//! non-`rust` or `rust,ignore` blocks, so rustdoc skips them.

use crate::paths;
use anyhow::{Context, Result};
use mdbook::MDBook;
use mdbook::book::BookItem;
use std::process::Command;

/// `cargo xtask test`: build the `rstv` lib, then run the guide's doctests.
pub fn run() -> Result<()> {
    // 1. Build the `rstv` lib so its rlib (and dependency rlibs) exist on
    //    disk. `-j2` respects the shared-machine core cap. Cargo produces a
    //    stable, unhashed `<target>/debug/librstv.rlib` for the lib target.
    let status = Command::new("cargo")
        .args(["build", "--package", "rstv", "--lib", "-j2"])
        .current_dir(paths::workspace_root())
        .status()
        .context("spawn cargo build -p rstv")?;
    anyhow::ensure!(status.success(), "cargo build -p rstv failed");

    let target = paths::target_dir();
    let rlib = target.join("debug").join("librstv.rlib");
    anyhow::ensure!(
        rlib.exists(),
        "rstv rlib not found at {} (did the build emit a lib?)",
        rlib.display()
    );
    let deps = target.join("debug").join("deps");
    let extern_arg = format!("rstv={}", rlib.to_str().context("rlib path is not UTF-8")?);

    // 2. Enumerate chapter source files via mdBook (respects SUMMARY.md; skips
    //    draft chapters that have no source path).
    let book = MDBook::load(paths::book_root()).map_err(|e| anyhow::anyhow!("load book: {e}"))?;
    let src = paths::book_root().join("src");

    let mut failures = Vec::new();
    let mut tested = 0usize;
    for item in book.book.iter() {
        let BookItem::Chapter(ch) = item else {
            continue;
        };
        let Some(rel) = ch.source_path.as_ref().or(ch.path.as_ref()) else {
            continue; // draft chapter — nothing to compile
        };
        let md = src.join(rel);

        // 3. rustdoc extracts and compiles the `rust` blocks itself; `--extern`
        //    makes `rstv` resolvable, `-L deps` covers its transitive deps.
        let status = Command::new("rustdoc")
            .arg("--test")
            .arg(&md)
            .args(["--edition", "2024"])
            .arg("--extern")
            .arg(&extern_arg)
            .arg("-L")
            .arg(&deps)
            .current_dir(paths::workspace_root())
            .status()
            .with_context(|| format!("spawn rustdoc for {}", md.display()))?;
        tested += 1;
        if !status.success() {
            failures.push(md);
        }
    }

    anyhow::ensure!(
        failures.is_empty(),
        "guide doctests failed in {} chapter(s): {}",
        failures.len(),
        failures
            .iter()
            .map(|p| p.display().to_string())
            .collect::<Vec<_>>()
            .join(", ")
    );
    eprintln!("OK: guide doctests passed ({tested} chapters checked)");
    Ok(())
}
