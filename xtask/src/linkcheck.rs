// TEMPORARY: removed in Task 4 once build.rs calls check_tree.
#![allow(dead_code)]

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
