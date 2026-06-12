//! Data-driven, process-global keymap shared by the editor and input line.
//!
//! Models the VS Code keybindings shape — a chord (1–2 keystrokes) maps to a
//! `Command` by name. Generalizes the C++ editor's `firstKeys`/`quickKeys`/
//! `blockKeys` tables and `key_state` prefix machine. See
//! `docs/superpowers/specs/2026-06-12-configurable-keymap-design.md`.
//!
//! **Guide:** [Keyboard & key mapping](../../../apps/keyboard.html).

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

    pub(crate) fn normalize(key: Key, ctrl: bool, alt: bool, shift: bool) -> Self {
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
        other => {
            if let Some(Ok(n)) = other.strip_prefix('f').map(str::parse::<u8>) {
                return Ok(Key::F(n));
            }
            let mut chars = other.chars();
            if let (Some(c), None) = (chars.next(), chars.clone().next()) {
                return Ok(Key::Char(c));
            }
            return Err(format!("unknown key name {other:?}"));
        }
    })
}

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

#[cfg(test)]
pub(crate) static GLOBAL_KEYMAP_TEST_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

/// RAII guard for tests that mutate the process-global keymap: serializes
/// against other global-touching tests and restores `word_star()` on drop
/// (even on panic). Use in any test that calls `set_global`.
#[cfg(test)]
pub(crate) struct GlobalKeymapGuard(
    // Held purely for its RAII effect: keeps the serialization lock until drop.
    #[allow(dead_code)] std::sync::MutexGuard<'static, ()>,
);

#[cfg(test)]
impl GlobalKeymapGuard {
    pub(crate) fn new(km: Keymap) -> Self {
        let lock = GLOBAL_KEYMAP_TEST_LOCK
            .lock()
            .unwrap_or_else(|e| e.into_inner()); // tolerate a poisoned lock from a prior panic
        set_global(km);
        GlobalKeymapGuard(lock)
    }
}

#[cfg(test)]
impl Drop for GlobalKeymapGuard {
    fn drop(&mut self) {
        set_global(Keymap::word_star());
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

    #[test]
    fn parse_single_stroke() {
        let c = parse_chord("ctrl+c").unwrap();
        assert_eq!(c.0.len(), 1);
        assert_eq!(
            c.0[0],
            KeyStroke::normalize(Key::Char('c'), true, false, false)
        );
    }

    #[test]
    fn parse_named_and_modifiers() {
        assert_eq!(
            parse_chord("shift+insert").unwrap().0[0],
            KeyStroke::normalize(Key::Insert, false, false, true)
        );
        assert_eq!(
            parse_chord("alt+backspace").unwrap().0[0],
            KeyStroke::normalize(Key::Backspace, false, true, false)
        );
        assert_eq!(
            parse_chord("f5").unwrap().0[0],
            KeyStroke::normalize(Key::F(5), false, false, false)
        );
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
        // The "default is word_star / Backspace → BACK_SPACE" property is a
        // property of word_star() itself — assert it locally, without touching
        // the process-global (so this part can't race other tests).
        let local = Keymap::word_star();
        let bs = KeyStroke::from_event(ev(Key::Backspace, false, false, false));
        assert!(matches!(local.resolve(None, bs), Resolve::Command(c) if c == Command::BACK_SPACE));

        // The set_global / resolve_global round-trip mutates shared state, so
        // run it under the serializing guard (restores word_star() on drop).
        let _g = GlobalKeymapGuard::new(Keymap::cua());
        let cc = KeyStroke::from_event(ev(Key::Char('c'), true, false, false));
        assert!(matches!(resolve_global(None, cc), Resolve::Command(c) if c == Command::COPY));
    }

    #[test]
    fn word_star_transcribes_editor_tables_plus_backspace_fix() {
        let km = Keymap::word_star();
        let r = |k: Key, ctrl, alt, shift| {
            km.resolve(None, KeyStroke::from_event(ev(k, ctrl, alt, shift)))
        };
        // The bug fix:
        assert!(
            matches!(r(Key::Backspace, false, false, false), Resolve::Command(c) if c == Command::BACK_SPACE)
        );
        // A representative diamond binding, a named key, and a prefix:
        assert!(
            matches!(r(Key::Char('s'), true, false, false), Resolve::Command(c) if c == Command::CHAR_LEFT)
        );
        assert!(
            matches!(r(Key::Char('a'), true, false, false), Resolve::Command(c) if c == Command::SELECT_ALL)
        );
        assert!(matches!(
            r(Key::Char('q'), true, false, false),
            Resolve::Prefix
        ));
        assert!(
            matches!(r(Key::Enter, false, false, false), Resolve::Command(c) if c == Command::NEW_LINE)
        );
        // Ctrl-Q F → FIND (quickKeys prefix).
        let q = KeyStroke::from_event(ev(Key::Char('q'), true, false, false));
        let f = KeyStroke::from_event(ev(Key::Char('f'), false, false, false));
        assert!(matches!(km.resolve(Some(q), f), Resolve::Command(c) if c == Command::FIND));
    }

    #[test]
    fn cua_and_emacs_core_bindings() {
        let cua = Keymap::cua();
        let r = |km: &Keymap, k: Key, ctrl| {
            km.resolve(None, KeyStroke::from_event(ev(k, ctrl, false, false)))
        };
        assert!(matches!(r(&cua, Key::Char('c'), true), Resolve::Command(c) if c == Command::COPY));
        assert!(
            matches!(r(&cua, Key::Char('v'), true), Resolve::Command(c) if c == Command::PASTE)
        );
        assert!(matches!(r(&cua, Key::Char('z'), true), Resolve::Command(c) if c == Command::UNDO));

        let em = Keymap::emacs();
        assert!(
            matches!(r(&em, Key::Char('a'), true), Resolve::Command(c) if c == Command::LINE_START)
        );
        assert!(
            matches!(r(&em, Key::Char('e'), true), Resolve::Command(c) if c == Command::LINE_END)
        );
    }
}
