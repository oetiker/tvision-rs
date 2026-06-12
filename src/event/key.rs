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
//! and `KeyDownEvent` (`system.h`). `TKey`'s canonical form is already
//! decomposed into a base code plus a modifier mask (`kbCtrlA == TKey('A',
//! kbCtrlShift)`, `kbShiftTab == TKey(kbTab, kbShift)`); rstv keeps that
//! decomposition with the [`Key`] enum and a separate [`KeyModifiers`]
//! (deviations D4 and D5).

/// A physical key.
///
/// Only the base, modifier-free keys appear here. Combined `kb*` codes such as
/// `kbCtrlA`, `kbShiftTab`, `kbAltF3` or `kbCtrlEnter` are *not* variants; they
/// decompose into one of these plus the matching [`KeyModifiers`] flags, exactly
/// as `TKey` normalizes them.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum Key {
    /// A printable character: letters, digits, symbols, and space
    /// (`kbSpace` is `Char(' ')`). Holds the base character; case and the
    /// Ctrl/Alt modifiers live in [`KeyModifiers`].
    Char(char),
    /// A function key, `F(1)`..=`F(12)` (`kbF1`..`kbF12`).
    F(u8),
    /// `kbEnter` / `kbCtrlEnter` (base key).
    Enter,
    /// `kbEsc` / `kbAltEsc` (base key).
    Esc,
    /// `kbBack` (Backspace) / `kbCtrlBack` / `kbAltBack` (base key).
    Backspace,
    /// `kbTab` / `kbShiftTab` / `kbCtrlTab` / `kbAltTab` (base key). Note there
    /// is no `BackTab` variant: Shift+Tab is `Tab` + the `shift` modifier.
    Tab,
    /// `kbUp` / `kbCtrlUp` / `kbAltUp` (base key).
    Up,
    /// `kbDown` / `kbCtrlDown` / `kbAltDown` (base key).
    Down,
    /// `kbLeft` / `kbCtrlLeft` / `kbAltLeft` (base key).
    Left,
    /// `kbRight` / `kbCtrlRight` / `kbAltRight` (base key).
    Right,
    /// `kbHome` / `kbCtrlHome` / `kbAltHome` (base key).
    Home,
    /// `kbEnd` / `kbCtrlEnd` / `kbAltEnd` (base key).
    End,
    /// `kbPgUp` / `kbCtrlPgUp` / `kbAltPgUp` (base key).
    PageUp,
    /// `kbPgDn` / `kbCtrlPgDn` / `kbAltPgDn` (base key).
    PageDown,
    /// `kbIns` / `kbCtrlIns` / `kbShiftIns` / `kbAltIns` (base key).
    Insert,
    /// `kbDel` / `kbCtrlDel` / `kbShiftDel` / `kbAltDel` (base key).
    Delete,
}

/// The active keyboard modifiers. Replaces the `controlKeyState` bit-word
/// (`tkeys.h` `kb*Shift` masks, `system.h`) with a struct-of-bools (deviation
/// D5).
///
/// Only the three logical modifiers `TKey` itself tracks are modeled; the
/// platform left/right-Ctrl, left/right-Alt and left/right-Shift distinctions
/// (`kbLeftCtrl` vs `kbRightCtrl`, etc.) collapse into a single flag each, just
/// as `kbCtrlShift`/`kbAltShift`/`kbShift` do in the C++.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash)]
pub struct KeyModifiers {
    /// Shift is held (`kbShift`).
    pub shift: bool,
    /// Ctrl is held (`kbCtrlShift`).
    pub ctrl: bool,
    /// Alt is held (`kbAltShift`).
    pub alt: bool,
}

/// A key-down event: a physical [`Key`] together with the [`KeyModifiers`]
/// active when it was pressed. Faithful to `KeyDownEvent` (`system.h`) and to
/// `TKey`'s `{code, mods}` pair, in the decomposed form (see the [module
/// docs](self)).
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

/// Extract the hotkey character from a label string — faithful port of
/// `hotKey`/`hotKeyStr` (`tinputli.cpp`).
///
/// The convention: the first `~`-delimited run in `s` marks the hotkey. The
/// char immediately following the first `~` is the hotkey, **uppercased**.
/// Returns `None` if there is no `~`, if the `~` is at the end of the string
/// (nothing after it), or if the char after `~` is itself `~` (a
/// tilde-escape). The C++ `hotKeyStr` returns the substring after `~` up to
/// the next `~` (or end), and `hotKey` takes the first char of that substring;
/// both return a null/empty result for the no-`~` case.
///
/// Deviation: the C++ returns `'\0'` rather than a sentinel; we return
/// `Option<char>` idiomatically. The `"~~x"` guard (char after `~` is `~`) is
/// **faithful**, not an addition: C++ `hotKeyStr` calls `memchr` for the *next*
/// `~` starting at `begin` (the char right after the first `~`); for `"~~x"`
/// that finds the second `~` immediately, so the returned substring is empty
/// and `hotKey` returns `'\0'`. Our `None` matches that exactly.
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

/// Map WordStar Ctrl-letter navigation keys to their arrow/nav equivalents —
/// faithful port of `ctrlToArrow` (`drivers2.cpp`).
///
/// In the decomposed key model, a Ctrl-letter is `Key::Char(letter)` with
/// `modifiers.ctrl` set. The 11-entry table from the C++
/// (`ctrlCodes[]`→`arrowCodes[]`) maps:
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
/// The C++ `keyCode != 0` guard is auto-satisfied in our model (`kbNoKey` has
/// no `Key` variant — there is never a null `KeyDown`), so no equivalent guard
/// is needed here.
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

/// Test whether a key event is the Alt-accelerator for a given hotkey char —
/// the decomposed successor of the C++ idiom
/// `event.keyDown.keyCode == getAltCode(c)` (`tbutton.cpp:218`,
/// `tcluster.cpp:262`, `tlabel.cpp:93`).
///
/// Together with [`is_plain_hotkey`] (and optionally [`ctrl_to_arrow`] for
/// WordStar nav pre-processing), these predicates replace the C++ accelerator
/// idiom seen in `tbutton.cpp:191`, `tcluster.cpp:256`, and `tlabel.cpp`.
/// Callers compose them with the phase/focus gate.
///
/// Returns `true` iff:
/// - `ke.modifiers.alt` is set, **and**
/// - `ke.key` is `Key::Char(c)` where `c` matches `hot` case-insensitively.
///
/// **Deviation — broader than `getAltCode`:** C++ `getAltCode` is a whitelist
/// (`altCodes1`/`altCodes2` in `tvtext2.cpp`) that only maps `A–Z` and
/// `0–9`, `-`, `=`, returning 0 (never matches) for any other char. We instead
/// accept any `Alt+Char` case-insensitively; the difference is unobservable in
/// practice because hotkeys are always letters or digits. The C++ Alt-Space
/// special case (`altSpaceChar = '\xF0'` → `kbAltSpace`) is dropped.
///
/// Only `alt` is required; `shift` and `ctrl` are not checked. In the C++,
/// Alt keys produce a scan-code with `charScan.charCode == 0`, so only the
/// alt-scan-code path matches — the behavior is the same.
///
/// **Predicate, not the keycode:** every C++ consumer of `getAltCode` uses it
/// only as `keyCode == getAltCode(c)` (verified at all three sites above), so a
/// predicate is the faithful shape. The one caller that needs the *value*,
/// `getCtrlCode` (`tvtext2.cpp:136`), has no port yet; add a value-returning
/// `alt_code(c) -> Key` if/when a consumer needs it.
pub fn is_alt_hotkey(ke: &KeyEvent, hot: char) -> bool {
    if !ke.modifiers.alt {
        return false;
    }
    matches!(ke.key, Key::Char(c) if c.eq_ignore_ascii_case(&hot))
}

/// Test whether a key event is a plain (non-modified) hotkey press — the
/// decomposed successor of the C++ plain-letter branch
/// `c == (char) toupper(event.keyDown.charScan.charCode)` (`tbutton.cpp`,
/// `tcluster.cpp`).
///
/// Returns `true` iff:
/// - neither `alt` nor `ctrl` is held, **and**
/// - `ke.key` is `Key::Char(c)` where `c` matches `hot` case-insensitively.
///
/// **Why `!alt && !ctrl` is required (faithful, not cosmetic):** the C++ branch
/// compares against `charScan.charCode`, which is the *plain ASCII* code of the
/// keystroke. When Alt is held that field is `0`; when Ctrl is held it is the
/// control code (e.g. Ctrl+S → `0x13`), never the letter. So the C++ plain
/// branch can only fire for an unmodified letter. In our decomposed model
/// (`CLAUDE.md`) Ctrl+S is `Key::Char('s')` + `ctrl`, so omitting the `ctrl`
/// check would let `is_plain_hotkey(Ctrl+S, 'S')` false-match — a real
/// divergence the `||` composition does **not** mask (Ctrl makes
/// [`is_alt_hotkey`] false, so the call falls through to this predicate).
/// Guarding both modifiers also makes the function match its name ("plain").
/// **Callers still own the phase gate** (postProcess / focused), not this
/// helper.
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
        // kbSpace is just Char(' ').
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
        // The decomposition contract: there is no BackTab variant. magiblot's
        // kbShiftTab == TKey(kbTab, kbShift).
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
        // kbCtrlC == TKey('C', kbCtrlShift); we model the base char + ctrl.
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
        // kbAltF3 decomposes into F(3) + alt.
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
        // Faithful: C++ compares against charScan.charCode, which under Ctrl is
        // the control code (Ctrl+O != 'O'), so the plain branch never fires.
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
        // Under Alt, C++ charScan.charCode is 0, so the plain branch never
        // fires; the Alt match is is_alt_hotkey's job.
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
