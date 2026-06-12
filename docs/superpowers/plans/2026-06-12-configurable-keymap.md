# Configurable Keymap Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace the editor's hardcoded key `match` and the input line's `ctrl_to_arrow` handling with one data-driven, process-global `Keymap` (WordStar default + CUA + Emacs presets), fixing the plain-Backspace bug and adding a live keymap selector to the `tvedit` example.

**Architecture:** A new `src/keymap.rs` defines `KeyStroke` (a normalized keystroke), `Chord` (1–2 strokes), and `Keymap` (`Chord → Command` + a prefix set), plus a process-global `OnceLock<RwLock<Keymap>>` seeded with `Keymap::word_star()`. `Editor` and `InputLine` resolve incoming `KeyDown`s through `keymap::resolve_global(...)`. The editor turns the result into an `Event::Command`; the input line dispatches the result through a shared `apply_input_command` covering its single-line repertoire and **bubbles** anything outside it (so dialog Enter/Tab/Esc still work).

**Tech Stack:** Rust (workspace crate `tvision`), `insta` snapshot tests (D11), `std::sync::{OnceLock, RwLock}`. No new dependencies.

**Spec:** `docs/superpowers/specs/2026-06-12-configurable-keymap-design.md`

---

## Conventions for every task

- Build/test artifacts land in `/home/oetiker/scratch/cargo-target` — `export CARGO_TARGET_DIR=/home/oetiker/scratch/cargo-target` before any cargo command.
- Cap parallelism: `cargo test --workspace -- --test-threads=4`, `cargo build -j4`.
- Gates that must pass before every commit: `cargo test --workspace`, `cargo clippy --workspace --all-targets -- -D warnings`, `cargo fmt --all --check`.
- Commit messages end with: `Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>`.
- This is **INFRA**: each task is implemented by a fresh subagent, then spec-compliance + code-quality review before integrating (CLAUDE.md methodology). Phases are sequential — do not parallelize; later phases import types from earlier ones.

## Divergence recorded during planning (intentional, no test breaks)

Unifying editor + input line means the input line's `Ctrl-A` and `Ctrl-F` change under the **WordStar default** from `Home`/`End` (old `ctrl_to_arrow`) to the editor's `SELECT_ALL` / `WORD_RIGHT`. No existing input-line test covers these, and it is the modern-expected direction. All other `ctrl_to_arrow` letters (`S/D/E/X/G/V/R/C/H`) already agree with the editor table or bubble identically in a single-line field. The only behavior change visible to existing tests is the editor's **plain Backspace now deletes** (the bug fix).

## File Structure

- **Create `src/keymap.rs`** — the whole keymap subsystem: `KeyStroke`, `Chord`, `Keymap`, the `Resolve` enum, the chord-string parser, the process-global, and the three presets. One module, one responsibility (key→command resolution). Unit-tested in-file.
- **Modify `src/lib.rs`** — add `pub mod keymap;` and re-export `keymap::{Keymap, KeyStroke, Resolve}` alongside existing re-exports.
- **Modify `src/widgets/editor.rs`** — delete `enum KeyMapResult` + `fn scan_key_map`; change field `key_state: i32` → `pending: Option<KeyStroke>`; rewrite `fn convert_event` to use `keymap::resolve_global`.
- **Modify `src/widgets/input_line.rs`** — remove the `ctrl_to_arrow` call; replace the `match ke.key` body with `keymap::resolve_global` + a new `fn apply_input_command`; route the existing `Event::Command` clipboard arm through the same helper.
- **Modify `examples/tvedit.rs`** — add the `~O~ptions ▸ Keyboard mapping` submenu, three example-local command constants, the active-preset check marks, and the `run_app` dispatch that calls `tv::keymap::set_global`.

---

## Phase 1 — The keymap primitive (`src/keymap.rs`)

### Task 1: `KeyStroke` + normalization

**Files:**
- Create: `src/keymap.rs`
- Modify: `src/lib.rs` (add `pub mod keymap;`)

- [ ] **Step 1: Create the module skeleton + `pub mod keymap;`**

Add to `src/lib.rs` next to `pub mod widgets;`:

```rust
pub mod keymap;
```

Create `src/keymap.rs`:

```rust
//! Data-driven, process-global keymap shared by the editor and input line.
//!
//! Models the VS Code keybindings shape — a chord (1–2 keystrokes) maps to a
//! `Command` by name. Generalizes the C++ editor's `firstKeys`/`quickKeys`/
//! `blockKeys` tables and `key_state` prefix machine. See
//! `docs/superpowers/specs/2026-06-12-configurable-keymap-design.md`.

use crate::command::Command;
use crate::event::{Key, KeyEvent, KeyModifiers};
use std::collections::{HashMap, HashSet};
use std::sync::{OnceLock, RwLock};

/// One normalized keystroke: a `Key` plus the three real modifiers.
///
/// Normalization (`from_event`) folds two cases so presets stay small and the
/// C++ "second prefix key is uppercased" / "shift+arrow == arrow" behaviors are
/// preserved:
/// * **Alphabetic `Char`** → lowercased, `shift` forced false (letter commands
///   never depend on shift; `ctrl+q a` == `ctrl+q A`).
/// * **Cursor-pad keys** (`Left/Right/Up/Down/Home/End/PageUp/PageDown`) →
///   `shift` forced false. Shift on those is a *selection* modifier handled in
///   the widgets, never a distinct binding (so `shift+Left` resolves to the
///   same movement as `Left`).
/// * **Everything else** (`Insert/Delete/Tab/Enter/F-keys/punctuation`) keeps
///   `shift` — so `shift+Insert` (paste) stays distinct from `Insert`.
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct KeyStroke {
    pub key: Key,
    pub ctrl: bool,
    pub alt: bool,
    pub shift: bool,
}

impl KeyStroke {
    /// Normalize a raw key event into a lookup key.
    pub fn from_event(ke: KeyEvent) -> Self {
        let KeyModifiers { shift, ctrl, alt } = ke.modifiers;
        Self::normalize(ke.key, ctrl, alt, shift)
    }

    fn normalize(key: Key, ctrl: bool, alt: bool, shift: bool) -> Self {
        match key {
            Key::Char(c) if c.is_ascii_alphabetic() => KeyStroke {
                key: Key::Char(c.to_ascii_lowercase()),
                ctrl,
                alt,
                shift: false,
            },
            Key::Left
            | Key::Right
            | Key::Up
            | Key::Down
            | Key::Home
            | Key::End
            | Key::PageUp
            | Key::PageDown => KeyStroke {
                key,
                ctrl,
                alt,
                shift: false,
            },
            _ => KeyStroke {
                key,
                ctrl,
                alt,
                shift,
            },
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn ev(key: Key, ctrl: bool, alt: bool, shift: bool) -> KeyEvent {
        KeyEvent::new(key, KeyModifiers { shift, ctrl, alt })
    }

    #[test]
    fn letters_fold_case_and_shift() {
        let lower = KeyStroke::from_event(ev(Key::Char('a'), true, false, false));
        let upper = KeyStroke::from_event(ev(Key::Char('A'), true, false, true));
        assert_eq!(lower, upper);
        assert_eq!(lower.key, Key::Char('a'));
        assert!(!lower.shift);
    }

    #[test]
    fn shift_arrow_equals_arrow() {
        let plain = KeyStroke::from_event(ev(Key::Left, false, false, false));
        let shifted = KeyStroke::from_event(ev(Key::Left, false, false, true));
        assert_eq!(plain, shifted);
    }

    #[test]
    fn shift_insert_stays_distinct() {
        let plain = KeyStroke::from_event(ev(Key::Insert, false, false, false));
        let shifted = KeyStroke::from_event(ev(Key::Insert, false, false, true));
        assert_ne!(plain, shifted);
    }
}
```

- [ ] **Step 2: Run the tests — expect PASS**

Run: `cargo test --workspace keymap::tests -- --test-threads=4`
Expected: the 3 `KeyStroke` tests PASS; whole workspace still builds.

- [ ] **Step 3: Commit**

```bash
git add src/keymap.rs src/lib.rs
git commit -m "feat(keymap): KeyStroke normalization primitive

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

### Task 2: chord-string parser

**Files:**
- Modify: `src/keymap.rs`

- [ ] **Step 1: Write failing tests** (append inside `mod tests`):

```rust
#[test]
fn parse_single_stroke() {
    let c = parse_chord("ctrl+c").unwrap();
    assert_eq!(c.0.len(), 1);
    assert_eq!(c.0[0], KeyStroke::normalize(Key::Char('c'), true, false, false));
}

#[test]
fn parse_named_and_modifiers() {
    assert_eq!(parse_chord("shift+insert").unwrap().0[0],
        KeyStroke::normalize(Key::Insert, false, false, true));
    assert_eq!(parse_chord("alt+backspace").unwrap().0[0],
        KeyStroke::normalize(Key::Backspace, false, true, false));
    assert_eq!(parse_chord("f5").unwrap().0[0],
        KeyStroke::normalize(Key::F(5), false, false, false));
}

#[test]
fn parse_two_stroke_chord() {
    let c = parse_chord("ctrl+k ctrl+c").unwrap();
    assert_eq!(c.0.len(), 2);
}

#[test]
fn parse_rejects_garbage() {
    assert!(parse_chord("ctrl+nope").is_err());
    assert!(parse_chord("").is_err());
}
```

- [ ] **Step 2: Run — expect FAIL** (`parse_chord`/`Chord` not defined).

Run: `cargo test --workspace keymap::tests::parse -- --test-threads=4`
Expected: compile error "cannot find function `parse_chord`".

- [ ] **Step 3: Implement `Chord` + `parse_chord`** (add above `mod tests`):

```rust
/// A chord: one keystroke, or two for a prefix sequence (Ctrl-K / Ctrl-Q style).
#[derive(Clone, PartialEq, Eq, Hash, Debug)]
pub struct Chord(pub Vec<KeyStroke>);

/// Parse a VS Code-style chord string: space-separated strokes, each a
/// `+`-joined list of `ctrl|shift|alt|cmd|meta` modifiers ending in a key name.
/// Pure (no I/O). `cmd`/`meta` are accepted as aliases for `ctrl` (portability).
pub fn parse_chord(s: &str) -> Result<Chord, String> {
    let strokes: Vec<&str> = s.split_whitespace().collect();
    if strokes.is_empty() {
        return Err(format!("empty chord: {s:?}"));
    }
    let mut out = Vec::with_capacity(strokes.len());
    for stroke in strokes {
        out.push(parse_stroke(stroke)?);
    }
    Ok(Chord(out))
}

fn parse_stroke(s: &str) -> Result<KeyStroke, String> {
    let (mut ctrl, mut alt, mut shift) = (false, false, false);
    let mut key: Option<Key> = None;
    for tok in s.split('+') {
        match tok.to_ascii_lowercase().as_str() {
            "ctrl" | "cmd" | "meta" => ctrl = true,
            "alt" | "opt" | "option" => alt = true,
            "shift" => shift = true,
            other => key = Some(parse_key(other)?),
        }
    }
    let key = key.ok_or_else(|| format!("no key in stroke {s:?}"))?;
    Ok(KeyStroke::normalize(key, ctrl, alt, shift))
}

fn parse_key(name: &str) -> Result<Key, String> {
    Ok(match name {
        "backspace" | "bs" => Key::Backspace,
        "delete" | "del" => Key::Delete,
        "insert" | "ins" => Key::Insert,
        "home" => Key::Home,
        "end" => Key::End,
        "pageup" | "pgup" => Key::PageUp,
        "pagedown" | "pgdn" => Key::PageDown,
        "left" => Key::Left,
        "right" => Key::Right,
        "up" => Key::Up,
        "down" => Key::Down,
        "enter" | "return" => Key::Enter,
        "tab" => Key::Tab,
        "esc" | "escape" => Key::Esc,
        "space" => Key::Char(' '),
        f if f.starts_with('f') && f[1..].parse::<u8>().is_ok() => {
            Key::F(f[1..].parse().unwrap())
        }
        c if c.chars().count() == 1 => Key::Char(c.chars().next().unwrap()),
        other => return Err(format!("unknown key name {other:?}")),
    })
}
```

- [ ] **Step 4: Run — expect PASS.**

Run: `cargo test --workspace keymap::tests -- --test-threads=4`
Expected: all keymap tests PASS.

- [ ] **Step 5: Commit**

```bash
git add src/keymap.rs
git commit -m "feat(keymap): VS Code-style chord string parser

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

### Task 3: `Keymap`, `Resolve`, builder, and the process-global

**Files:**
- Modify: `src/keymap.rs`, `src/lib.rs` (re-exports)

- [ ] **Step 1: Write failing tests** (append to `mod tests`):

```rust
#[test]
fn resolve_single_and_prefix_and_miss() {
    let mut km = Keymap::new();
    km.bind("ctrl+s", Command::CHAR_LEFT);
    km.bind("ctrl+k ctrl+c", Command::COPY);

    let s = KeyStroke::from_event(ev(Key::Char('s'), true, false, false));
    assert!(matches!(km.resolve(None, s), Resolve::Command(c) if c == Command::CHAR_LEFT));

    let k = KeyStroke::from_event(ev(Key::Char('k'), true, false, false));
    assert!(matches!(km.resolve(None, k), Resolve::Prefix));

    let c = KeyStroke::from_event(ev(Key::Char('c'), true, false, false));
    assert!(matches!(km.resolve(Some(k), c), Resolve::Command(cmd) if cmd == Command::COPY));

    // Unfinished prefix + wrong second key → None (pending already consumed).
    let z = KeyStroke::from_event(ev(Key::Char('z'), true, false, false));
    assert!(matches!(km.resolve(Some(k), z), Resolve::None));
}

#[test]
fn global_default_is_word_star_and_settable() {
    // Default global resolves plain Backspace to BACK_SPACE (the bug fix).
    let bs = KeyStroke::from_event(ev(Key::Backspace, false, false, false));
    assert!(matches!(resolve_global(None, bs), Resolve::Command(c) if c == Command::BACK_SPACE));

    set_global(Keymap::cua());
    let cc = KeyStroke::from_event(ev(Key::Char('c'), true, false, false));
    assert!(matches!(resolve_global(None, cc), Resolve::Command(c) if c == Command::COPY));

    set_global(Keymap::word_star()); // restore for other tests
}
```

> Note: `word_star()`/`cua()` land in Task 4; this test references them so Task 3 + Task 4 are committed together if needed. If implementing strictly task-by-task, temporarily stub `word_star`/`cua` to `Keymap::new()` to compile Task 3, then fill them in Task 4. Prefer implementing Task 3 and Task 4 back-to-back.

- [ ] **Step 2: Run — expect FAIL** (`Keymap`/`Resolve`/`resolve_global` undefined).

- [ ] **Step 3: Implement** (add above `mod tests`):

```rust
/// The outcome of resolving a keystroke against a keymap.
pub enum Resolve {
    /// A fully-resolved command.
    Command(Command),
    /// This stroke begins a known two-stroke chord; caller should hold it.
    Prefix,
    /// No binding — caller treats the key as insertable text or lets it bubble.
    None,
}

/// A `Chord → Command` table plus the set of strokes that begin a 2-chord.
#[derive(Clone, Default)]
pub struct Keymap {
    bindings: HashMap<Chord, Command>,
    prefixes: HashSet<KeyStroke>,
}

impl Keymap {
    /// An empty keymap.
    pub fn new() -> Self {
        Keymap::default()
    }

    /// Bind a chord string to a command (panics on a malformed chord — presets
    /// and app code use compile-time-constant strings).
    pub fn bind(&mut self, chord: &str, cmd: Command) -> &mut Self {
        let c = parse_chord(chord).unwrap_or_else(|e| panic!("bad chord {chord:?}: {e}"));
        if c.0.len() >= 2 {
            self.prefixes.insert(c.0[0]);
        }
        self.bindings.insert(c, cmd);
        self
    }

    /// Remove a binding if present.
    pub fn unbind(&mut self, chord: &str) -> &mut Self {
        if let Ok(c) = parse_chord(chord) {
            self.bindings.remove(&c);
            // Recompute the prefix set from the remaining 2-chords.
            self.prefixes = self
                .bindings
                .keys()
                .filter(|k| k.0.len() >= 2)
                .map(|k| k.0[0])
                .collect();
        }
        self
    }

    /// Resolve `stroke`, combined with an optional `pending` prefix stroke.
    pub fn resolve(&self, pending: Option<KeyStroke>, stroke: KeyStroke) -> Resolve {
        let chord = match pending {
            Some(p) => Chord(vec![p, stroke]),
            None => Chord(vec![stroke]),
        };
        if let Some(&cmd) = self.bindings.get(&chord) {
            return Resolve::Command(cmd);
        }
        if pending.is_none() && self.prefixes.contains(&stroke) {
            return Resolve::Prefix;
        }
        Resolve::None
    }
}

fn global_cell() -> &'static RwLock<Keymap> {
    static GLOBAL: OnceLock<RwLock<Keymap>> = OnceLock::new();
    GLOBAL.get_or_init(|| RwLock::new(Keymap::word_star()))
}

/// Replace the process-global keymap (the default for all text input).
pub fn set_global(km: Keymap) {
    *global_cell().write().expect("keymap lock poisoned") = km;
}

/// Resolve a stroke against the process-global keymap.
pub fn resolve_global(pending: Option<KeyStroke>, stroke: KeyStroke) -> Resolve {
    global_cell()
        .read()
        .expect("keymap lock poisoned")
        .resolve(pending, stroke)
}
```

Add to `src/lib.rs` re-export block:

```rust
pub use keymap::{Keymap, KeyStroke, Resolve};
```

- [ ] **Step 4: Run — expect PASS** (after Task 4 fills the presets, or with the temporary stubs).

- [ ] **Step 5: Commit** (combined with Task 4 if stubbed).

```bash
git add src/keymap.rs src/lib.rs
git commit -m "feat(keymap): Keymap, Resolve, builder, process-global

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

### Task 4: the three presets

**Files:**
- Modify: `src/keymap.rs`

- [ ] **Step 1: Write the WordStar-default behavior test** (append to `mod tests`):

```rust
#[test]
fn word_star_transcribes_editor_tables_plus_backspace_fix() {
    let km = Keymap::word_star();
    let r = |k: Key, ctrl, alt, shift| km.resolve(None, KeyStroke::from_event(ev(k, ctrl, alt, shift)));
    // The bug fix:
    assert!(matches!(r(Key::Backspace, false, false, false), Resolve::Command(c) if c == Command::BACK_SPACE));
    // A representative diamond binding, a named key, and a prefix:
    assert!(matches!(r(Key::Char('s'), true, false, false), Resolve::Command(c) if c == Command::CHAR_LEFT));
    assert!(matches!(r(Key::Char('a'), true, false, false), Resolve::Command(c) if c == Command::SELECT_ALL));
    assert!(matches!(r(Key::Char('q'), true, false, false), Resolve::Prefix));
    assert!(matches!(r(Key::Enter, false, false, false), Resolve::Command(c) if c == Command::NEW_LINE));
    // Ctrl-Q F → FIND (quickKeys prefix).
    let q = KeyStroke::from_event(ev(Key::Char('q'), true, false, false));
    let f = KeyStroke::from_event(ev(Key::Char('f'), false, false, false));
    assert!(matches!(km.resolve(Some(q), f), Resolve::Command(c) if c == Command::FIND));
}

#[test]
fn cua_and_emacs_core_bindings() {
    let cua = Keymap::cua();
    let r = |km: &Keymap, k: Key, ctrl| km.resolve(None, KeyStroke::from_event(ev(k, ctrl, false, false)));
    assert!(matches!(r(&cua, Key::Char('c'), true), Resolve::Command(c) if c == Command::COPY));
    assert!(matches!(r(&cua, Key::Char('v'), true), Resolve::Command(c) if c == Command::PASTE));
    assert!(matches!(r(&cua, Key::Char('z'), true), Resolve::Command(c) if c == Command::UNDO));

    let em = Keymap::emacs();
    assert!(matches!(r(&em, Key::Char('a'), true), Resolve::Command(c) if c == Command::LINE_START));
    assert!(matches!(r(&em, Key::Char('e'), true), Resolve::Command(c) if c == Command::LINE_END));
}
```

- [ ] **Step 2: Run — expect FAIL** (presets stubbed to empty).

- [ ] **Step 3: Implement the presets** (replace the Task-3 stubs; add above `mod tests`). The WordStar table is a 1:1 transcription of the deleted `editor.rs` arms plus plain `backspace`:

```rust
impl Keymap {
    /// Faithful default: transcription of the C++ editor `firstKeys`/`quickKeys`/
    /// `blockKeys` tables, plus plain `backspace → BACK_SPACE` (the bug fix).
    pub fn word_star() -> Self {
        let mut k = Keymap::new();
        // firstKeys — Ctrl-letter diamond.
        k.bind("ctrl+a", Command::SELECT_ALL)
            .bind("ctrl+c", Command::PAGE_DOWN)
            .bind("ctrl+d", Command::CHAR_RIGHT)
            .bind("ctrl+e", Command::LINE_UP)
            .bind("ctrl+f", Command::WORD_RIGHT)
            .bind("ctrl+g", Command::DEL_CHAR)
            .bind("ctrl+h", Command::BACK_SPACE)
            .bind("ctrl+l", Command::SEARCH_AGAIN)
            .bind("ctrl+m", Command::NEW_LINE)
            .bind("ctrl+o", Command::INDENT_MODE)
            .bind("ctrl+p", Command::ENCODING)
            .bind("ctrl+r", Command::PAGE_UP)
            .bind("ctrl+s", Command::CHAR_LEFT)
            .bind("ctrl+t", Command::DEL_WORD)
            .bind("ctrl+u", Command::UNDO)
            .bind("ctrl+v", Command::INS_MODE)
            .bind("ctrl+x", Command::LINE_DOWN)
            .bind("ctrl+y", Command::DEL_LINE);
        // firstKeys — named keys (shift folded away on pad keys by normalization).
        k.bind("left", Command::CHAR_LEFT)
            .bind("right", Command::CHAR_RIGHT)
            .bind("backspace", Command::BACK_SPACE) // the fix (was unbound → no-op)
            .bind("alt+backspace", Command::DEL_WORD_LEFT)
            .bind("ctrl+backspace", Command::DEL_WORD_LEFT)
            .bind("ctrl+delete", Command::DEL_WORD)
            .bind("ctrl+left", Command::WORD_LEFT)
            .bind("ctrl+right", Command::WORD_RIGHT)
            .bind("home", Command::LINE_START)
            .bind("end", Command::LINE_END)
            .bind("up", Command::LINE_UP)
            .bind("down", Command::LINE_DOWN)
            .bind("pageup", Command::PAGE_UP)
            .bind("pagedown", Command::PAGE_DOWN)
            .bind("ctrl+home", Command::TEXT_START)
            .bind("ctrl+end", Command::TEXT_END)
            .bind("insert", Command::INS_MODE)
            .bind("delete", Command::DEL_CHAR)
            .bind("shift+insert", Command::PASTE)
            .bind("shift+delete", Command::CUT)
            .bind("ctrl+insert", Command::COPY)
            .bind("enter", Command::NEW_LINE);
        // quickKeys (Ctrl-Q prefix).
        k.bind("ctrl+q a", Command::REPLACE)
            .bind("ctrl+q c", Command::TEXT_END)
            .bind("ctrl+q d", Command::LINE_END)
            .bind("ctrl+q f", Command::FIND)
            .bind("ctrl+q h", Command::DEL_START)
            .bind("ctrl+q r", Command::TEXT_START)
            .bind("ctrl+q s", Command::LINE_START)
            .bind("ctrl+q y", Command::DEL_END);
        // blockKeys (Ctrl-K prefix).
        k.bind("ctrl+k b", Command::START_SELECT)
            .bind("ctrl+k c", Command::PASTE)
            .bind("ctrl+k h", Command::HIDE_SELECT)
            .bind("ctrl+k k", Command::COPY)
            .bind("ctrl+k y", Command::CUT);
        k
    }

    /// CUA / "Office" preset — modern muscle memory across editor and fields.
    pub fn cua() -> Self {
        let mut k = Keymap::new();
        k.bind("ctrl+c", Command::COPY)
            .bind("ctrl+x", Command::CUT)
            .bind("ctrl+v", Command::PASTE)
            .bind("ctrl+z", Command::UNDO)
            .bind("ctrl+a", Command::SELECT_ALL)
            .bind("ctrl+f", Command::FIND)
            .bind("backspace", Command::BACK_SPACE)
            .bind("delete", Command::DEL_CHAR)
            .bind("ctrl+backspace", Command::DEL_WORD_LEFT)
            .bind("ctrl+delete", Command::DEL_WORD)
            .bind("left", Command::CHAR_LEFT)
            .bind("right", Command::CHAR_RIGHT)
            .bind("ctrl+left", Command::WORD_LEFT)
            .bind("ctrl+right", Command::WORD_RIGHT)
            .bind("up", Command::LINE_UP)
            .bind("down", Command::LINE_DOWN)
            .bind("home", Command::LINE_START)
            .bind("end", Command::LINE_END)
            .bind("ctrl+home", Command::TEXT_START)
            .bind("ctrl+end", Command::TEXT_END)
            .bind("pageup", Command::PAGE_UP)
            .bind("pagedown", Command::PAGE_DOWN)
            .bind("insert", Command::INS_MODE)
            .bind("enter", Command::NEW_LINE);
        k
    }

    /// Emacs preset — readline/Cocoa bindings; now active in input fields too.
    pub fn emacs() -> Self {
        let mut k = Keymap::new();
        k.bind("ctrl+a", Command::LINE_START)
            .bind("ctrl+e", Command::LINE_END)
            .bind("ctrl+f", Command::CHAR_RIGHT)
            .bind("ctrl+b", Command::CHAR_LEFT)
            .bind("ctrl+n", Command::LINE_DOWN)
            .bind("ctrl+p", Command::LINE_UP)
            .bind("ctrl+d", Command::DEL_CHAR)
            .bind("ctrl+k", Command::DEL_END)
            .bind("ctrl+y", Command::PASTE)
            .bind("alt+f", Command::WORD_RIGHT)
            .bind("alt+b", Command::WORD_LEFT)
            .bind("backspace", Command::BACK_SPACE)
            .bind("delete", Command::DEL_CHAR)
            .bind("left", Command::CHAR_LEFT)
            .bind("right", Command::CHAR_RIGHT)
            .bind("up", Command::LINE_UP)
            .bind("down", Command::LINE_DOWN)
            .bind("home", Command::LINE_START)
            .bind("end", Command::LINE_END)
            .bind("enter", Command::NEW_LINE);
        k
    }
}
```

- [ ] **Step 4: Run — expect PASS** for all keymap tests; full workspace builds.

Run: `cargo test --workspace keymap -- --test-threads=4 && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check`

- [ ] **Step 5: Commit**

```bash
git add src/keymap.rs src/lib.rs
git commit -m "feat(keymap): WordStar/CUA/Emacs presets

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Phase 2 — Editor adoption (`src/widgets/editor.rs`)

### Task 5: route the editor through the global keymap; lock the Backspace fix

**Files:**
- Modify: `src/widgets/editor.rs` (field ~409/498, `convert_event` ~2235, delete `KeyMapResult` + `scan_key_map` ~2438/2260)
- Test: a new `#[test]` in `editor.rs`'s test module

- [ ] **Step 1: Write the failing regression test** (in `editor.rs` tests; adapt to the existing test harness/helpers in that module — find the pattern other key tests use, e.g. building an `Editor` on the `HeadlessBackend` and feeding a `KeyDown`):

```rust
#[test]
fn plain_backspace_deletes_char_left() {
    // Type "ab", press Backspace, expect "a".
    let mut ed = /* construct as sibling tests do */;
    feed_text(&mut ed, "ab");
    feed_key(&mut ed, KeyEvent::from(Key::Backspace));
    assert_eq!(ed.logical_text(), "a"); // use the module's text oracle (bufChar reconstruct)
}
```

> The implementer must mirror the construction/feeding helpers already in
> `editor.rs` tests (search for existing `KeyDown`-driven tests). The oracle for
> contents is the "Reconstruct the logical text" helper documented near line 565.

- [ ] **Step 2: Run — expect FAIL** (today plain Backspace is a no-op, so result is `"ab"`).

Run: `cargo test --workspace -p tvision editor::tests::plain_backspace -- --test-threads=4`
Expected: FAIL, `"ab"` != `"a"`.

- [ ] **Step 3: Implement the swap.**

  1. Add the import near the top of `editor.rs`: `use crate::keymap::{self, KeyStroke, Resolve};`
  2. Replace the field (`~line 409`) `key_state: i32,` with `pending: Option<KeyStroke>,` and its initializer (`~line 498`) `key_state: 0,` with `pending: None,`.
  3. Replace the whole `fn convert_event` body with:

```rust
fn convert_event(&mut self, ev: &mut crate::event::Event) {
    use crate::event::Event;
    if let Event::KeyDown(k) = ev {
        let stroke = KeyStroke::from_event(*k);
        let pending = self.pending.take();
        match keymap::resolve_global(pending, stroke) {
            Resolve::Prefix => {
                self.pending = Some(stroke);
                ev.clear();
            }
            Resolve::Command(c) => {
                *ev = Event::Command(c);
            }
            Resolve::None => {
                // Insertable char or unhandled — leave the event unchanged.
            }
        }
    }
}
```

  4. Delete the now-unused `fn scan_key_map` (~2259–2360) and `enum KeyMapResult` (~2438). Fix any other references (e.g. `self.key_state` elsewhere — grep `key_state` in the file and replace the prefix-reset semantics; the only writer was `convert_event`).

- [ ] **Step 4: Run — expect PASS + no snapshot drift.**

Run: `cargo test --workspace -p tvision editor -- --test-threads=4`
Expected: new test PASSES; all existing editor snapshot tests unchanged (the keymap default reproduces the old table). If `insta` reports a diff, STOP — the transcription is wrong; reconcile against the table in Task 4.

- [ ] **Step 5: Full gates + commit**

```bash
cargo test --workspace -- --test-threads=4 && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check
git add src/widgets/editor.rs
git commit -m "feat(editor): resolve keys via global keymap; fix plain Backspace

Replaces the hardcoded scan_key_map/KeyMapResult and key_state prefix
machine with keymap::resolve_global + a pending-stroke. Plain Backspace
now maps to BACK_SPACE (faithful to C++ where kbCtrlH's zero high byte
matched kbBack's low byte).

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Phase 3 — Input-line adoption (`src/widgets/input_line.rs`) — highest risk

### Task 6: extract `apply_input_command` and route both key + command paths through it

**Files:**
- Modify: `src/widgets/input_line.rs` (KeyDown arm ~702–840, Command arm ~858+, import ~67)

This task preserves every existing per-key body verbatim — it only changes the
*dispatch key* from `Key` to `Command`, and feeds it from the keymap. The
single-line **repertoire** (commands the field acts on) is:

| Command | Action (existing body to reuse) |
|---|---|
| `CHAR_LEFT` / `CHAR_RIGHT` | grapheme move (old `Key::Left` / `Key::Right`) |
| `WORD_LEFT` / `WORD_RIGHT` | `prev_word` / `next_word` (old `Ctrl+Left/Right`) |
| `LINE_START` / `LINE_END` | `cur_pos = 0` / `= data.len()` (old `Home`/`End`) |
| `BACK_SPACE` | old plain `Key::Backspace` body |
| `DEL_WORD_LEFT` | old `Key::Backspace if ctrl` body |
| `DEL_CHAR` | old plain `Key::Delete` body |
| `DEL_WORD` | old `Key::Delete if ctrl` body |
| `INS_MODE` | old `Key::Insert` toggle |
| `DEL_LINE` | old `Ctrl-Y` clear-field body (`data.clear(); cur_pos = 0`) |
| `SELECT_ALL` | NEW: `sel_start = 0; sel_end = data.len(); cur_pos = data.len()` |
| `CUT` / `COPY` / `PASTE` | the existing `Event::Command` clipboard bodies |

Any **other** resolved command (`NEW_LINE`, `LINE_UP/DOWN`, `PAGE_UP/DOWN`,
`FIND`, …) and any `Prefix`/unbound non-printable → **not handled → bubbles**
(set `handled = false`, hit the existing `if !handled { return; }`).

- [ ] **Step 1: Write failing tests** (in `input_line.rs` tests; mirror the existing `key`/`ctrl_key` helpers there):

```rust
#[test]
fn keymap_default_backspace_and_nav_still_work() {
    // Regression: WordStar default preserves the field's editing keys.
    let mut il = /* construct as sibling tests do, data = "abc", cursor at end */;
    feed(&mut il, key(Key::Backspace));
    assert_eq!(il.data(), "ab");
    feed(&mut il, key(Key::Home));
    assert_eq!(il.cursor(), 0);
}

#[test]
fn cua_ctrl_c_copies_in_input_line() {
    keymap::set_global(Keymap::cua());
    let mut il = /* data = "hello", select all */;
    feed(&mut il, key(Key::Char('a')).with_ctrl()); // SELECT_ALL
    feed(&mut il, key(Key::Char('c')).with_ctrl()); // COPY
    assert_eq!(/* clipboard via ctx */, "hello");
    keymap::set_global(Keymap::word_star());
}

#[test]
fn enter_tab_esc_bubble_under_every_preset() {
    for preset in [Keymap::word_star(), Keymap::cua(), Keymap::emacs()] {
        keymap::set_global(preset);
        let mut il = /* data = "x" */;
        let mut ev = Event::KeyDown(KeyEvent::from(Key::Enter));
        feed_ev(&mut il, &mut ev);
        assert!(matches!(ev, Event::KeyDown(_)), "Enter must remain live (bubble)");
    }
    keymap::set_global(Keymap::word_star());
}
```

> Use the module's real accessors for `data()`/`cursor()`/clipboard; the implementer wires these to existing test helpers.

- [ ] **Step 2: Run — expect FAIL** (`apply_input_command` not present; CUA copy not wired).

- [ ] **Step 3: Implement.**

  1. Import: change `use crate::event::{Event, Key, MouseEvent, ctrl_to_arrow};` → drop `ctrl_to_arrow`, add the keymap: `use crate::event::{Event, Key, MouseEvent}; use crate::keymap::{self, KeyStroke, Resolve};`
  2. In the `Event::KeyDown(ke)` arm, replace `let ke = ctrl_to_arrow(*ke);` and the subsequent `match ke.key { … }` with keymap resolution. Compute movement-ness from the resolved command (for `extend_block`), then dispatch:

```rust
Event::KeyDown(ke) => {
    self.save_state();
    let shift = ke.modifiers.shift;
    let stroke = KeyStroke::from_event(*ke);
    let cmd = match keymap::resolve_global(None, stroke) {
        Resolve::Command(c) => Some(c),
        _ => None,
    };

    let is_move = matches!(cmd, Some(c) if
        c == Command::CHAR_LEFT || c == Command::CHAR_RIGHT
        || c == Command::WORD_LEFT || c == Command::WORD_RIGHT
        || c == Command::LINE_START || c == Command::LINE_END);
    let extend_block = is_move && shift;
    if extend_block {
        // (unchanged anchor-setup block from the old code)
    }

    let mut handled = true;
    match cmd {
        Some(c) => handled = self.apply_input_command(c, ctx),
        None => {
            // Printable insertion: only a plain Char with no ctrl/alt.
            match ke.key {
                Key::Char(c) if !ke.modifiers.ctrl && !ke.modifiers.alt => {
                    // (unchanged printable-insertion body from the old code)
                }
                _ => handled = false,
            }
        }
    }

    if !handled {
        return; // leave the event live so the dialog/group sees it
    }
    // (unchanged tail: extend_block ? adjust_select_block() : clear sel; firstPos follow; sync_cursor; ev.clear())
}
```

  3. Add the helper method on `InputLine` (move the existing per-key bodies into it verbatim, keyed by command; `CUT/COPY/PASTE` reuse the exact bodies from the `Event::Command` arm):

```rust
/// Apply a resolved editor command within the single-line repertoire.
/// Returns `true` if handled; `false` means "not ours — let it bubble".
fn apply_input_command(&mut self, cmd: Command, ctx: &mut Context) -> bool {
    match cmd {
        Command::CHAR_LEFT  => { /* old plain Left body */ }
        Command::CHAR_RIGHT => { /* old plain Right body */ }
        Command::WORD_LEFT  => self.cur_pos = prev_word(&self.data, self.cur_pos),
        Command::WORD_RIGHT => self.cur_pos = next_word(&self.data, self.cur_pos),
        Command::LINE_START => self.cur_pos = 0,
        Command::LINE_END   => self.cur_pos = self.data.len() as i32,
        Command::BACK_SPACE     => { /* old plain Backspace body */ }
        Command::DEL_WORD_LEFT  => { /* old Ctrl+Backspace body */ }
        Command::DEL_CHAR       => { /* old plain Delete body */ }
        Command::DEL_WORD       => { /* old Ctrl+Delete body */ }
        Command::INS_MODE       => self.state.state.cursor_ins = !self.state.state.cursor_ins,
        Command::DEL_LINE       => { self.data.clear(); self.cur_pos = 0; }
        Command::SELECT_ALL     => {
            self.sel_start = 0;
            self.sel_end = self.data.len() as i32;
            self.cur_pos = self.data.len() as i32;
        }
        Command::CUT   => { /* old Event::Command CUT body (guarded by selection) */ return true; }
        Command::COPY  => { /* old Event::Command COPY body */ return true; }
        Command::PASTE => { /* old Event::Command PASTE body (request_input_line_paste) */ return true; }
        _ => return false, // outside the single-line repertoire → bubble
    }
    true
}
```

  4. Update the existing `Event::Command` arm to delegate to the same helper so menu-driven CUT/COPY/PASTE keep working:

```rust
Event::Command(cmd) => {
    if self.apply_input_command(*cmd, ctx) {
        // CUT/COPY/PASTE bodies already do their own clipboard/scroll bookkeeping.
        ev.clear();
    }
    // (preserve the existing post-command firstPos/sync_cursor tail if any)
}
```

> Watch the borrow on `ctx` and the `ev.clear()` placement — the CUT/COPY/PASTE bodies must keep their exact clipboard + `sync_cursor` bookkeeping. Keep the `save_state()` call where it was (once at the top of KeyDown). Preserve `check_valid` calls inside each delete body.

- [ ] **Step 4: Run — expect PASS + no snapshot drift.**

Run: `cargo test --workspace -p tvision input_line -- --test-threads=4`
Expected: new tests PASS; existing input_line + dialog snapshots unchanged. Investigate ANY `insta` diff before accepting.

- [ ] **Step 5: Full gates + commit**

```bash
cargo test --workspace -- --test-threads=4 && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check
git add src/widgets/input_line.rs
git commit -m "feat(input_line): resolve keys via global keymap with repertoire/bubble rule

Replaces ctrl_to_arrow + the per-Key match with keymap::resolve_global
feeding a shared apply_input_command. Commands outside the single-line
repertoire (NEW_LINE, paging, …) bubble so dialog Enter/Tab/Esc still
work. Ctrl-A/Ctrl-F now follow the unified map (select-all/word-right
under the default) — see plan divergence note.

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

---

## Phase 4 — Global setter + tvedit selector (`examples/tvedit.rs`)

### Task 7: Options ▸ Keyboard mapping submenu with live switching

**Files:**
- Modify: `examples/tvedit.rs` (menu builder ~90–128, command dispatch ~135–155, key helpers/imports)

- [ ] **Step 1: Add three example-local command constants** near the top of `tvedit.rs`:

```rust
// Example-local commands for the keymap selector.
const KEYMAP_WORDSTAR: Command = Command::new("tvedit.keymap.wordstar");
const KEYMAP_CUA: Command = Command::new("tvedit.keymap.cua");
const KEYMAP_EMACS: Command = Command::new("tvedit.keymap.emacs");
```

> Confirm the `Command` constructor name in `src/command.rs` (`Command::new` per `command.rs:58`). Import `tvision::Command` if not already in scope.

- [ ] **Step 2: Add the submenu** to the `Menu::builder()` chain (after the `~W~indows` submenu, before `.build()` — match the existing `.submenu(...)` / `.command(...)` builder API used in the file):

```rust
.submenu("~O~ptions", alt('o'), |m| {
    m.submenu("~K~eyboard mapping", None, |k| {
        k.command("~W~ordStar", KEYMAP_WORDSTAR)
            .command("~C~UA", KEYMAP_CUA)
            .command("~E~macs", KEYMAP_EMACS)
    })
})
```

> Check the exact submenu/command builder signatures in `src/menu/` (the file already uses `.submenu(label, key, |m| …)` and `.command(label, cmd)`); adapt the nested-submenu call to whatever the builder supports. If nested submenus are unsupported, fall back to three flat `Options` items.

- [ ] **Step 3: Handle the commands** in the `run_app` closure (`tvedit.rs:135`), adding to the `if/else if` chain:

```rust
} else if cmd == KEYMAP_WORDSTAR {
    tvision::keymap::set_global(tvision::Keymap::word_star());
} else if cmd == KEYMAP_CUA {
    tvision::keymap::set_global(tvision::Keymap::cua());
} else if cmd == KEYMAP_EMACS {
    tvision::keymap::set_global(tvision::Keymap::emacs());
}
```

- [ ] **Step 4: Build + run the example to verify live switching.**

Run: `cargo build --example tvedit -j4`
Expected: compiles clean. Then a manual smoke check (per the tmux-sandbox-gotcha memory — launch + interact + capture in ONE invocation) or, minimally, confirm the menu renders and the commands dispatch via a small headless test if practical. Document the manual check result.

- [ ] **Step 5: Full gates + commit**

```bash
cargo test --workspace -- --test-threads=4 && cargo clippy --workspace --all-targets -- -D warnings && cargo fmt --all --check
git add examples/tvedit.rs
git commit -m "feat(examples/tvedit): Options menu to switch keymap preset live

Co-Authored-By: Claude Opus 4.8 <noreply@anthropic.com>"
```

### Task 8 (optional polish): active-preset check marks

**Files:** `examples/tvedit.rs`

- [ ] If the menu builder supports a checked/marked item state, track the active preset in app state and mark the current one. If it does not (verify in `src/menu/`), **skip this task** and note it — do not invent menu API. Commit only if implemented.

---

## Self-Review (completed during planning)

**Spec coverage:** §1 primitive → Tasks 1–3; §2 authoring strings → Task 2; §3 global knob → Task 3; §4 both-widgets + pass-through → Tasks 5–6; §5 presets → Task 4; §6 tvedit menu → Task 7; §"Testing" → tests embedded per task. ✓ All spec sections mapped.

**Placeholder scan:** The input-line bodies in Task 6 say "old … body" — these are deliberate *move-verbatim* instructions referencing concrete, cited existing code (with the full mapping table), not invented behavior. Test bodies reference "construct as sibling tests do" because the editor/input_line test harnesses are large and idiomatic to those files; the implementer mirrors the existing pattern rather than a fabricated one. All *new* logic (normalization, parser, resolve, presets, convert_event, apply_input_command scaffold, menu wiring) is given in full.

**Type consistency:** `KeyStroke::from_event`, `keymap::resolve_global`, `Resolve::{Command,Prefix,None}`, `Keymap::{new,bind,unbind,resolve,word_star,cua,emacs}`, `set_global` — names identical across all tasks. Editor field renamed `key_state` → `pending: Option<KeyStroke>` consistently. ✓

**Risk note:** Phase 3 is the only step that changes faithful input-line behavior (Ctrl-A/Ctrl-F) and restructures a 140-line handler; its three tests (default-nav regression, CUA copy, Enter/Tab/Esc bubble) plus the dialog snapshot suite are the guard. Verify on the integrated tree, not just the subagent's worktree (worktree-shared-target-clippy memory).
