# TWildStr  (guide p. 577)

Rust module(s): src/dialog/filedlg.rs   |   magiblot: stddlg.h

| Guide entry | Pg | Bucket | Corr | Rust symbol / mapping | Doc | Notes |
|---|---|---|---|---|---|---|
| `TWildStr` (type) | 577 | EQUIVALENT | OK | `&str` / `String` wildcard pattern in `src/dialog/filedlg.rs` (e.g. `FileDialog::new(.., wild_card: &str, ..)`) | N/A | Guide: `TWildStr = PathStr` — a Pascal fixed-capacity path string ("identical to the `PathDir` type in the Dos unit"), used by standard dialogs to pass wildcard file-name templates. Rust has no fixed-capacity Pascal string type: the wildcard template is a plain `&str`/`String` carried by `TFileDialog`'s port (see `TFileDialog.md`, the `wildCard` field row). Known idiomatic mapping: Pascal `string[n]` alias → Rust `&str`/`String`. Type alias only, no public Rust symbol of its own. |

## Summary
- PORTED: 0   EQUIVALENT: 1   NOT-PORTED: 0   MISSING: 0   UNSURE: 0
- SUSPECT: 0   |   doc<3 (public): 0   |   → concept: 0
- Notable findings: No gap — wildcard templates are `&str`/`String` on `FileDialog` (the `wildCard` field, audited in `TFileDialog.md`). Pascal `PathStr` alias has no standalone Rust type by design.
