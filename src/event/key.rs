//! Keyboard keys and key-down events — deviations **D4** and **D5**.
//!
//! Ports the `kb*` key-code family (`tkeys.h`), the `TKey` class
//! (`tkeys.h` / `tkey.cpp`), and the `KeyDownEvent` struct (`system.h`).
//!
//! **Decomposed model (matches magiblot's canonical `TKey`).** A survey of the
//! C++ established that `TKey`'s *canonical* form is already decomposed into a
//! base key code plus a separate modifier mask — `TKey` normalizes combined
//! codes so that `kbCtrlA == TKey('A', kbCtrlShift)` and
//! `kbShiftTab == TKey(kbTab, kbShift)` (see `tkey.cpp`). We therefore model a
//! keystroke the same idiomatic, crossterm-shaped way: a closed [`Key`] enum of
//! *physical* keys plus a *separate* [`KeyModifiers`] channel (the old
//! `controlKeyState` bit-word). There are deliberately **no** modifier-combined
//! variants: `Ctrl+C` is `Key::Char('c')` + `ctrl`, `Shift+Tab` is `Key::Tab` +
//! `shift`, `Alt+F3` is `Key::F(3)` + `alt`. This decomposition is the whole
//! point of the design.
//!
//! `kbNoKey` (`0x0000`) has no variant: it is represented by the *absence* of a
//! key event (no `KeyDownEvent` is produced at all).

/// A physical key. Faithful to the *base* keys of the `kb*` family
/// (`tkeys.h`), in the decomposed form `TKey` canonicalizes to — modifiers are
/// carried separately in [`KeyModifiers`], never folded into a variant.
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

/// The active keyboard modifiers — deviation **D5**, replacing the
/// `controlKeyState` bit-word (`tkeys.h` `kb*Shift` masks, `system.h`).
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
}
