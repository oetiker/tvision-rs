//! Keyboard keys and key-down events.
//!
//! A keystroke is modeled the idiomatic, crossterm-shaped way: a closed [`Key`]
//! enum of *physical* keys plus a *separate* [`KeyModifiers`] channel. There are
//! deliberately **no** modifier-combined variants: `Ctrl+C` is `Key::Char('c')`
//! plus `ctrl`, `Shift+Tab` is `Key::Tab` plus `shift`, and `Alt+F3` is
//! `Key::F(3)` plus `alt`. A [`KeyEvent`] pairs a key with the modifiers active
//! when it was pressed.
//!
//! There is no "no key" value: the absence of a keystroke is the absence of a
//! key event.
//!
//! # Turbo Vision heritage
//!
//! Ports the `kb*` key-code family, the `TKey` class (`tkeys.h` / `tkey.cpp`),
//! and `KeyDownEvent` (`system.h`). `TKey`'s canonical form already decomposes
//! each keystroke into a base code plus a modifier mask (Ctrl+A is `'A'` with the
//! ctrl-shift mask; Shift+Tab is the Tab code with the shift mask); rstv keeps
//! that decomposition as the [`Key`] enum and a separate [`KeyModifiers`]
//! (deviations D4 and D5).

/// A physical key.
///
/// Only the base, modifier-free keys appear here. Modifier combinations such as
/// Ctrl+A, Shift+Tab, Alt+F3 or Ctrl+Enter are *not* variants; they decompose
/// into one of these plus the matching [`KeyModifiers`] flags.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    /// A printable character: letters, digits, symbols, and space (`Char(' ')`).
    /// Holds the base character; case and the Ctrl/Alt modifiers live in
    /// [`KeyModifiers`].
    Char(char),
    /// A function key, `F(1)`..=`F(12)`.
    F(u8),
    /// The Enter/Return key.
    Enter,
    /// The Escape key.
    Esc,
    /// The Backspace key.
    Backspace,
    /// The Tab key. Note there is no `BackTab` variant: Shift+Tab is `Tab` + the
    /// `shift` modifier.
    Tab,
    /// The Up arrow.
    Up,
    /// The Down arrow.
    Down,
    /// The Left arrow.
    Left,
    /// The Right arrow.
    Right,
    /// The Home key.
    Home,
    /// The End key.
    End,
    /// The Page Up key.
    PageUp,
    /// The Page Down key.
    PageDown,
    /// The Insert key.
    Insert,
    /// The Delete key.
    Delete,
}

/// The active keyboard modifiers — a struct of three named `bool` flags.
///
/// Only the three logical modifiers are modeled; the platform left/right-Ctrl,
/// left/right-Alt and left/right-Shift distinctions collapse into a single flag
/// each.
///
/// # Turbo Vision heritage
/// The packed `controlKeyState` bit-word becomes a struct-of-bools (deviation
/// D5); the left/right modifier-side bits are folded into one flag each.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct KeyModifiers {
    /// Shift is held.
    pub shift: bool,
    /// Ctrl is held.
    pub ctrl: bool,
    /// Alt is held.
    pub alt: bool,
}

/// A key-down event: a physical [`Key`] together with the [`KeyModifiers`]
/// active when it was pressed (see the [module docs](self) for the decomposed
/// form).
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub struct KeyEvent {
    /// The physical key pressed.
    pub key: Key,
    /// The modifiers held while it was pressed.
    pub modifiers: KeyModifiers,
}

impl KeyEvent {
    /// A key event with explicit modifiers.
    pub fn new(key: Key, modifiers: KeyModifiers) -> Self {
        KeyEvent { key, modifiers }
    }
}

/// The common no-modifier case: `KeyEvent::from(Key::Enter)` yields an event
/// with all modifiers cleared.
impl From<Key> for KeyEvent {
    fn from(key: Key) -> Self {
        KeyEvent {
            key,
            modifiers: KeyModifiers::default(),
        }
    }
}

/// Extract the hotkey character from a label string.
///
/// The convention: the first `~`-delimited run in `s` marks the hotkey. The
/// char immediately following the first `~` is the hotkey, **uppercased**.
/// Returns `None` if there is no `~`, if the `~` is at the end of the string
/// (nothing after it), or if the char after `~` is itself `~` (a tilde-escape,
/// i.e. a literal `~` in the label).
///
/// # Turbo Vision heritage
/// Ports `hotKey`/`hotKeyStr` (`tinputli.cpp`); they returned a null char for the
/// no-hotkey case, where rstv returns `Option<char>`.
///
/// # Examples
///
/// ```
/// use tvision::event::hot_key;
/// assert_eq!(hot_key("~o~k"), Some('O'));  // uppercased
/// assert_eq!(hot_key("O~k"), Some('K'));  // first char after first '~'
/// assert_eq!(hot_key("No tilde here"), None);
/// assert_eq!(hot_key("trailing~"), None);
/// assert_eq!(hot_key("~~x"), None);       // char after '~' is '~'
/// ```
pub fn hot_key(s: &str) -> Option<char> {
    // Split on the first '~'; the right side is everything after it.
    let (_, after) = s.split_once('~')?;
    // Take the first char after '~'.
    let ch = after.chars().next()?;
    // If it is itself '~', treat as an escaped tilde — no hotkey.
    if ch == '~' {
        return None;
    }
    Some(ch.to_ascii_uppercase())
}

/// Map WordStar Ctrl-letter navigation keys to their arrow/nav equivalents.
///
/// In the decomposed key model, a Ctrl-letter is `Key::Char(letter)` with
/// `modifiers.ctrl` set. The 11-entry table maps:
///
/// | Ctrl+ | → Key |
/// |-------|-------|
/// | S     | Left  |
/// | D     | Right |
/// | E     | Up    |
/// | X     | Down  |
/// | A     | Home  |
/// | F     | End   |
/// | G     | Delete |
/// | V     | Insert |
/// | R     | PageUp |
/// | C     | PageDown |
/// | H     | Backspace |
///
/// The letter is matched case-insensitively. On a match the resulting
/// [`KeyEvent`] has **all modifiers cleared** (only the base arrow/nav key is
/// returned). Any key that does not match — including literal arrow keys,
/// non-Char keys, or Char keys without `ctrl` — is returned unchanged.
///
/// # Turbo Vision heritage
/// Ports `ctrlToArrow` (`drivers2.cpp`).
pub fn ctrl_to_arrow(ke: KeyEvent) -> KeyEvent {
    if !ke.modifiers.ctrl {
        return ke;
    }
    let mapped = match ke.key {
        Key::Char(c) => match c.to_ascii_lowercase() {
            's' => Some(Key::Left),
            'd' => Some(Key::Right),
            'e' => Some(Key::Up),
            'x' => Some(Key::Down),
            'a' => Some(Key::Home),
            'f' => Some(Key::End),
            'g' => Some(Key::Delete),
            'v' => Some(Key::Insert),
            'r' => Some(Key::PageUp),
            'c' => Some(Key::PageDown),
            'h' => Some(Key::Backspace),
            _ => None,
        },
        _ => None,
    };
    match mapped {
        Some(key) => KeyEvent {
            key,
            modifiers: KeyModifiers::default(),
        },
        None => ke,
    }
}

/// Test whether a key event is the Alt-accelerator for a given hotkey char.
///
/// Together with [`is_plain_hotkey`] (and optionally [`ctrl_to_arrow`] for
/// WordStar nav pre-processing), these predicates implement hotkey-accelerator
/// matching for buttons, clusters, and labels. Callers compose them with the
/// phase/focus gate.
///
/// Returns `true` iff:
/// - `ke.modifiers.alt` is set, **and**
/// - `ke.key` is `Key::Char(c)` where `c` matches `hot` case-insensitively.
///
/// Any `Alt+Char` is accepted case-insensitively; in practice hotkeys are always
/// letters or digits. Only `alt` is required; `shift` and `ctrl` are not checked.
///
/// # Turbo Vision heritage
/// Replaces the `event.keyDown.keyCode == getAltCode(c)` accelerator idiom
/// (`tbutton.cpp`, `tcluster.cpp`, `tlabel.cpp`). The original `getAltCode` was a
/// whitelist of `A–Z`, `0–9`, `-`, `=`; this predicate is deliberately broader.
/// The Alt-Space special case is dropped.
pub fn is_alt_hotkey(ke: &KeyEvent, hot: char) -> bool {
    if !ke.modifiers.alt {
        return false;
    }
    matches!(ke.key, Key::Char(c) if c.eq_ignore_ascii_case(&hot))
}

/// Test whether a key event is a plain (non-modified) hotkey press.
///
/// Returns `true` iff:
/// - neither `alt` nor `ctrl` is held, **and**
/// - `ke.key` is `Key::Char(c)` where `c` matches `hot` case-insensitively.
///
/// **Why `!alt && !ctrl` is required:** in the decomposed key model Ctrl+S is
/// `Key::Char('s')` + `ctrl`, so omitting the `ctrl` check would let
/// `is_plain_hotkey(Ctrl+S, 'S')` false-match. Since Ctrl makes [`is_alt_hotkey`]
/// false, the call falls through to this predicate, so guarding both modifiers is
/// what keeps a plain-letter press distinct from a modified one. **Callers still
/// own the phase gate** (post-process / focused), not this helper.
pub fn is_plain_hotkey(ke: &KeyEvent, hot: char) -> bool {
    if ke.modifiers.alt || ke.modifiers.ctrl {
        return false;
    }
    matches!(ke.key, Key::Char(c) if c.eq_ignore_ascii_case(&hot))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn char_variant_holds_letters_digits_and_space() {
        assert_eq!(Key::Char('a'), Key::Char('a'));
        assert_ne!(Key::Char('a'), Key::Char('b'));
        // Space is just Char(' ').
        assert_eq!(Key::Char(' '), Key::Char(' '));
        // Digits and symbols are ordinary characters too.
        assert_ne!(Key::Char('1'), Key::Char('!'));
    }

    #[test]
    fn function_keys_span_f1_to_f12() {
        assert_eq!(Key::F(1), Key::F(1));
        assert_ne!(Key::F(1), Key::F(2));
        // F11/F12 exist (kbF11/kbF12 in tkeys.h).
        let _ = Key::F(11);
        let _ = Key::F(12);
    }

    #[test]
    fn named_keys_are_distinct() {
        let keys = [
            Key::Enter,
            Key::Esc,
            Key::Backspace,
            Key::Tab,
            Key::Up,
            Key::Down,
            Key::Left,
            Key::Right,
            Key::Home,
            Key::End,
            Key::PageUp,
            Key::PageDown,
            Key::Insert,
            Key::Delete,
        ];
        for (i, a) in keys.iter().enumerate() {
            for (j, b) in keys.iter().enumerate() {
                assert_eq!(i == j, a == b);
            }
        }
    }

    #[test]
    fn no_modifier_constructor_clears_all_modifiers() {
        let ev = KeyEvent::from(Key::Enter);
        assert_eq!(ev.key, Key::Enter);
        assert!(!ev.modifiers.shift);
        assert!(!ev.modifiers.ctrl);
        assert!(!ev.modifiers.alt);
        assert_eq!(ev.modifiers, KeyModifiers::default());
    }

    #[test]
    fn new_carries_explicit_modifiers() {
        let mods = KeyModifiers {
            ctrl: true,
            ..Default::default()
        };
        let ev = KeyEvent::new(Key::Char('c'), mods);
        assert_eq!(ev.key, Key::Char('c'));
        assert_eq!(ev.modifiers, mods);
    }

    #[test]
    fn shift_tab_is_tab_plus_shift_not_a_backtab_variant() {
        // The decomposition contract: there is no BackTab variant — Shift+Tab is
        // the Tab key plus the shift modifier.
        let ev = KeyEvent::new(
            Key::Tab,
            KeyModifiers {
                shift: true,
                ..Default::default()
            },
        );
        assert_eq!(ev.key, Key::Tab);
        assert!(ev.modifiers.shift);
        assert!(!ev.modifiers.ctrl);
        assert!(!ev.modifiers.alt);
    }

    #[test]
    fn ctrl_c_is_char_plus_ctrl() {
        // Ctrl+C is modeled as the base char 'c' plus the ctrl modifier.
        let ev = KeyEvent::new(
            Key::Char('c'),
            KeyModifiers {
                ctrl: true,
                ..Default::default()
            },
        );
        assert_eq!(ev.key, Key::Char('c'));
        assert!(ev.modifiers.ctrl);
        assert!(!ev.modifiers.shift);
        assert!(!ev.modifiers.alt);
    }

    #[test]
    fn alt_f3_is_function_key_plus_alt() {
        // Alt+F3 decomposes into F(3) + alt.
        let ev = KeyEvent::new(
            Key::F(3),
            KeyModifiers {
                alt: true,
                ..Default::default()
            },
        );
        assert_eq!(ev.key, Key::F(3));
        assert!(ev.modifiers.alt);
        assert!(!ev.modifiers.ctrl);
        assert!(!ev.modifiers.shift);
    }

    // --- hot_key tests ---

    #[test]
    fn hot_key_tilde_delimited_returns_uppercase_first_char() {
        // Input uses a lowercase 'o' so the uppercasing step is actually exercised.
        assert_eq!(hot_key("~o~k"), Some('O'));
    }

    #[test]
    fn hot_key_char_after_first_tilde_uppercased() {
        // First char after the first '~' (which is 'k'), uppercased.
        assert_eq!(hot_key("O~k"), Some('K'));
    }

    #[test]
    fn hot_key_no_tilde_returns_none() {
        assert_eq!(hot_key("No tilde here"), None);
        assert_eq!(hot_key(""), None);
    }

    #[test]
    fn hot_key_trailing_tilde_returns_none() {
        assert_eq!(hot_key("trailing~"), None);
    }

    #[test]
    fn hot_key_double_tilde_escape_returns_none() {
        // The char immediately after the first '~' is another '~' — no hotkey.
        assert_eq!(hot_key("~~x"), None);
    }

    // --- ctrl_to_arrow tests ---

    fn ctrl_ev(ch: char) -> KeyEvent {
        KeyEvent::new(
            Key::Char(ch),
            KeyModifiers {
                ctrl: true,
                ..Default::default()
            },
        )
    }

    #[test]
    fn ctrl_to_arrow_s_maps_to_left_with_cleared_modifiers() {
        let result = ctrl_to_arrow(ctrl_ev('s'));
        assert_eq!(result.key, Key::Left);
        assert_eq!(result.modifiers, KeyModifiers::default());
    }

    #[test]
    fn ctrl_to_arrow_uppercase_s_also_maps_to_left() {
        let result = ctrl_to_arrow(ctrl_ev('S'));
        assert_eq!(result.key, Key::Left);
        assert_eq!(result.modifiers, KeyModifiers::default());
    }

    #[test]
    fn ctrl_to_arrow_h_maps_to_backspace() {
        let result = ctrl_to_arrow(ctrl_ev('h'));
        assert_eq!(result.key, Key::Backspace);
        assert_eq!(result.modifiers, KeyModifiers::default());
    }

    #[test]
    fn ctrl_to_arrow_r_maps_to_pageup() {
        let result = ctrl_to_arrow(ctrl_ev('r'));
        assert_eq!(result.key, Key::PageUp);
        assert_eq!(result.modifiers, KeyModifiers::default());
    }

    #[test]
    fn ctrl_to_arrow_d_maps_to_right() {
        let result = ctrl_to_arrow(ctrl_ev('d'));
        assert_eq!(result.key, Key::Right);
        assert_eq!(result.modifiers, KeyModifiers::default());
    }

    #[test]
    fn ctrl_to_arrow_literal_left_without_ctrl_passes_through() {
        let ke = KeyEvent::from(Key::Left);
        assert_eq!(ctrl_to_arrow(ke), ke);
    }

    #[test]
    fn ctrl_to_arrow_left_with_ctrl_passes_through() {
        // A literal arrow key with ctrl held is NOT a Char — must pass through.
        let ke = KeyEvent::new(
            Key::Left,
            KeyModifiers {
                ctrl: true,
                ..Default::default()
            },
        );
        assert_eq!(ctrl_to_arrow(ke), ke);
    }

    #[test]
    fn ctrl_to_arrow_char_s_without_ctrl_passes_through() {
        let ke = KeyEvent::from(Key::Char('s'));
        assert_eq!(ctrl_to_arrow(ke), ke);
    }

    #[test]
    fn ctrl_to_arrow_remaining_seven_mappings() {
        // Table-driven coverage for the 7 mappings not exercised by the tests above.
        let cases: &[(char, Key)] = &[
            ('e', Key::Up),
            ('x', Key::Down),
            ('a', Key::Home),
            ('f', Key::End),
            ('g', Key::Delete),
            ('v', Key::Insert),
            ('c', Key::PageDown),
        ];
        for &(ch, expected_key) in cases {
            let result = ctrl_to_arrow(ctrl_ev(ch));
            assert_eq!(
                result.key,
                expected_key,
                "Ctrl+{} should map to {:?}",
                ch.to_ascii_uppercase(),
                expected_key
            );
            assert_eq!(
                result.modifiers,
                KeyModifiers::default(),
                "Ctrl+{} mapping should clear all modifiers",
                ch.to_ascii_uppercase()
            );
        }
    }

    #[test]
    fn ctrl_to_arrow_unmapped_ctrl_letter_passes_through() {
        // Ctrl+'z' is not in the table; the event must be returned unchanged.
        let ke = ctrl_ev('z');
        let result = ctrl_to_arrow(ke);
        assert_eq!(result.key, Key::Char('z'), "key must remain Char('z')");
        assert!(result.modifiers.ctrl, "ctrl flag must still be set");
        assert_eq!(result, ke, "entire event must be unchanged");
    }

    // --- is_alt_hotkey tests ---

    #[test]
    fn is_alt_hotkey_alt_lower_matches_uppercase_hot() {
        let ke = KeyEvent::new(
            Key::Char('o'),
            KeyModifiers {
                alt: true,
                ..Default::default()
            },
        );
        assert!(is_alt_hotkey(&ke, 'O'));
    }

    #[test]
    fn is_alt_hotkey_alt_shift_also_matches() {
        let ke = KeyEvent::new(
            Key::Char('o'),
            KeyModifiers {
                alt: true,
                shift: true,
                ..Default::default()
            },
        );
        // Shift is not checked — alt+shift+o still matches hotkey 'O'.
        assert!(is_alt_hotkey(&ke, 'O'));
    }

    #[test]
    fn is_alt_hotkey_no_alt_returns_false() {
        let ke = KeyEvent::from(Key::Char('o'));
        assert!(!is_alt_hotkey(&ke, 'O'));
    }

    #[test]
    fn is_alt_hotkey_wrong_char_returns_false() {
        let ke = KeyEvent::new(
            Key::Char('x'),
            KeyModifiers {
                alt: true,
                ..Default::default()
            },
        );
        assert!(!is_alt_hotkey(&ke, 'O'));
    }

    // --- is_plain_hotkey tests ---

    #[test]
    fn is_plain_hotkey_matches_case_insensitively() {
        let ke = KeyEvent::from(Key::Char('o'));
        assert!(is_plain_hotkey(&ke, 'O'));
    }

    #[test]
    fn is_plain_hotkey_wrong_char_returns_false() {
        let ke = KeyEvent::from(Key::Char('x'));
        assert!(!is_plain_hotkey(&ke, 'O'));
    }

    #[test]
    fn is_plain_hotkey_ctrl_held_returns_false() {
        // A plain hotkey must be unmodified: Ctrl+O is not a plain 'O' press.
        let ke = KeyEvent::new(
            Key::Char('o'),
            KeyModifiers {
                ctrl: true,
                ..Default::default()
            },
        );
        assert!(!is_plain_hotkey(&ke, 'O'));
    }

    #[test]
    fn is_plain_hotkey_alt_held_returns_false() {
        // Under Alt this is not a plain press; the Alt match is is_alt_hotkey's job.
        let ke = KeyEvent::new(
            Key::Char('o'),
            KeyModifiers {
                alt: true,
                ..Default::default()
            },
        );
        assert!(!is_plain_hotkey(&ke, 'O'));
    }
}
