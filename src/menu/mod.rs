//! Menu data tree — `TMenuItem`, `TSubMenu`, `TMenu` (`menus.h`, `menu.cpp`).
//!
//! This module ports **only the menu data tree** — the pure node types plus a
//! builder API. The `TMenuView`/`TMenuBar`/`TMenuBox` views (drawing, event
//! handling, `execute`/`findItem`/`hotKey`) are later rows and live elsewhere.
//!
//! ## The C++ shape and how it maps
//!
//! In the C++, `TMenuItem` is a singly linked-list node carrying a C `union {
//! const char *param; TMenu *subMenu; }` discriminated *implicitly* by its
//! `name`/`command` fields. Consumers test, in order:
//!
//! - `name == 0` ⇒ a **separator** (`newLine()`: name=0, command=0, subMenu=0);
//! - else `command == 0` ⇒ a **submenu** (the union holds `subMenu`);
//! - else ⇒ a **command item** (the union holds `param`, the shortcut display
//!   text such as `"Alt-X"`).
//!
//! Per the house style (enums, like [`Key`] and
//! [`Event`](crate::event::Event)) we make that discrimination *explicit and
//! type-safe* with a 3-variant [`MenuItem`] enum: the `param`-xor-`subMenu`
//! union becomes the `Command`-vs-`SubMenu` choice, so an item can never hold
//! both. Shared fields (`name`, `key_code`, `help_ctx`, `disabled`) are read
//! uniformly via or-patterns, e.g.
//! `Command { disabled, .. } | SubMenu { disabled, .. } => …`.
//!
//! The C++ linked list (`next`) becomes a [`Vec`]; the `deflt` pointer becomes
//! [`Menu::default`], an *index* into that `Vec` (any valid index — the C++
//! `TMenu(itemList, TheDefault)` two-arg ctor allows a non-head default). The
//! [`MenuBuilder`] mirrors the common `TMenu(itemList)` case: it sets `default`
//! to `Some(0)` (the head) for a non-empty menu and `None` for an empty one.

use crate::command::Command;
use crate::event::{Key, KeyEvent, KeyModifiers};
use crate::help::HelpCtx;

pub mod menu_view;
pub use menu_view::MenuViewState;

/// A single menu entry. Ports `TMenuItem` (`menus.h`) — including the
/// `TSubMenu` subclass and `newLine()` separator — collapsed into one
/// type-safe enum.
///
/// The C++ `union { const char *param; TMenu *subMenu; }`, discriminated by the
/// `name`/`command` fields, becomes the three variants below: a [`Separator`]
/// (C++ `name == 0`), a [`Command`] item (C++ `command != 0`, union holds
/// `param`), and a [`SubMenu`] (C++ `command == 0`, union holds `subMenu`).
///
/// [`Separator`]: MenuItem::Separator
/// [`Command`]: MenuItem::Command
/// [`SubMenu`]: MenuItem::SubMenu
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuItem {
    /// A horizontal divider line. Ports C++ `newLine()` (`name == 0`).
    Separator,

    /// A command item. Ports a `TMenuItem` with `command != 0`, whose union
    /// holds `param` (the shortcut display text).
    Command {
        /// The `~`-marked label (C++ `name`).
        name: String,
        /// The command emitted when chosen (C++ `command`).
        command: Command,
        /// The accelerator key (C++ `keyCode`, a `TKey`). `None` is the C++
        /// `kbNoKey` — in our key model the absence of a key event.
        key_code: Option<KeyEvent>,
        /// Shortcut display text such as `"Alt-X"` (C++ `param`). `None` is the
        /// C++ `param == 0`.
        param: Option<String>,
        /// The help context (C++ `helpCtx`).
        help_ctx: HelpCtx,
        /// Whether the item is greyed out (C++ `disabled`). Mutated at runtime
        /// only on command items (command-graying, a later row).
        disabled: bool,
    },

    /// A submenu. Ports `TSubMenu` — a `TMenuItem` with `command == 0` whose
    /// union holds a fresh `subMenu`.
    SubMenu {
        /// The `~`-marked label (C++ `name`).
        name: String,
        /// The accelerator key (C++ `keyCode`). See [`MenuItem::Command`].
        key_code: Option<KeyEvent>,
        /// The help context (C++ `helpCtx`).
        help_ctx: HelpCtx,
        /// Whether the item is greyed out (C++ `disabled`).
        disabled: bool,
        /// The owned nested menu (C++ `subMenu`).
        menu: Menu,
    },
}

impl MenuItem {
    /// A mutable handle to the `disabled` flag, or `None` for a
    /// [`Separator`](MenuItem::Separator) (which has no such field).
    ///
    /// Mirrors the C++ runtime mutation of `TMenuItem::disabled`, which only
    /// ever targets command/submenu items, never a `newLine()`.
    pub fn disabled_mut(&mut self) -> Option<&mut bool> {
        match self {
            MenuItem::Separator => None,
            MenuItem::Command { disabled, .. } | MenuItem::SubMenu { disabled, .. } => {
                Some(disabled)
            }
        }
    }
}

/// A menu: an ordered list of [`MenuItem`]s plus the default selection. Ports
/// `TMenu` (`menus.h`).
///
/// The C++ linked list (`items`) becomes a [`Vec`]; the C++ `deflt` pointer
/// becomes [`default`](field@Menu::default), an *index* into that `Vec` — any
/// valid index, since the C++ `TMenu(itemList, TheDefault)` two-arg ctor allows
/// a non-head default. Both fields are public and not invariant-checked; the
/// [`MenuBuilder`] is what produces the common `Some(0)`-head / `None`-empty
/// convention (see its docs).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Menu {
    /// The menu entries, in order (C++ linked list `items`).
    pub items: Vec<MenuItem>,
    /// The index of the default item (C++ `deflt`); any valid index into
    /// [`items`], or `None` for no default. The [`MenuBuilder`] sets this to
    /// `Some(0)` (the head) for a non-empty menu and `None` for an empty one,
    /// but the field is unconstrained — the C++ two-arg ctor likewise permits a
    /// non-head default.
    ///
    /// [`items`]: Menu::items
    pub default: Option<usize>,
}

impl Menu {
    /// Start building a menu fluently. The successor of the C++ `operator+`
    /// chains over `TSubMenu`/`TMenuItem` (`menu.cpp`).
    pub fn builder() -> MenuBuilder {
        MenuBuilder::default()
    }
}

/// A fluent builder for a [`Menu`] — the idiomatic replacement for the C++
/// `operator+` overloads (`menu.cpp`) that chained `TSubMenu` and `TMenuItem`
/// nodes together.
///
/// Each method appends one [`MenuItem`] and returns `self` for chaining; nested
/// submenus are built with a closure. The first appended item sets
/// [`Menu::default`] to `Some(0)`, faithful to the C++ `TMenu(itemList)` /
/// empty-submenu `deflt = &head` behaviour.
#[derive(Default)]
pub struct MenuBuilder {
    menu: Menu,
}

impl MenuBuilder {
    /// Append an already-built [`MenuItem`] — the escape hatch for full control
    /// over `help_ctx`/`disabled` or any field the convenience methods don't
    /// expose.
    pub fn item(mut self, item: MenuItem) -> Self {
        self.menu.items.push(item);
        if self.menu.default.is_none() {
            self.menu.default = Some(0);
        }
        self
    }

    /// Append a separator line (C++ `newLine()`).
    pub fn separator(self) -> Self {
        self.item(MenuItem::Separator)
    }

    /// Append a command item with no accelerator and no shortcut text — the
    /// minimal `TMenuItem(name, command, kbNoKey)` form.
    pub fn command(self, name: impl Into<String>, command: Command) -> Self {
        self.item(MenuItem::Command {
            name: name.into(),
            command,
            key_code: None,
            param: None,
            help_ctx: HelpCtx::NO_CONTEXT,
            disabled: false,
        })
    }

    /// Append a command item with an accelerator and shortcut display text —
    /// the `TMenuItem(name, command, key, hcNoContext, param)` form.
    ///
    /// An empty `param` string is stored as `None`, faithful to C++ `param ==
    /// 0`.
    pub fn command_key(
        self,
        name: impl Into<String>,
        command: Command,
        key_code: impl Into<Option<KeyEvent>>,
        param: impl Into<String>,
    ) -> Self {
        let param = param.into();
        self.item(MenuItem::Command {
            name: name.into(),
            command,
            key_code: key_code.into(),
            param: if param.is_empty() { None } else { Some(param) },
            help_ctx: HelpCtx::NO_CONTEXT,
            disabled: false,
        })
    }

    /// Append a submenu, built by the closure. Ports the `TSubMenu(name, key)`
    /// node whose nested `TMenu` is filled by chained `operator+`.
    pub fn submenu(
        self,
        name: impl Into<String>,
        key_code: impl Into<Option<KeyEvent>>,
        build: impl FnOnce(MenuBuilder) -> MenuBuilder,
    ) -> Self {
        let menu = build(MenuBuilder::default()).build();
        self.item(MenuItem::SubMenu {
            name: name.into(),
            key_code: key_code.into(),
            help_ctx: HelpCtx::NO_CONTEXT,
            disabled: false,
            menu,
        })
    }

    /// Finish and produce the [`Menu`].
    pub fn build(self) -> Menu {
        self.menu
    }
}

/// Build an `Alt`+`<char>` accelerator — a menu-definition convenience that
/// mirrors the C++ `kbAltF`/`kbAltX`/… literals at the call site, keeping menu
/// trees terse.
pub fn alt(c: char) -> KeyEvent {
    KeyEvent::new(
        Key::Char(c),
        KeyModifiers {
            alt: true,
            ..Default::default()
        },
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    /// The builder must reproduce, node for node, the tree the C++ `operator+`
    /// chain produces for the canonical File/Window menu. The expected tree is
    /// hand-built with struct/enum literals (a *different* code path from the
    /// builder) so a builder bug cannot pass silently.
    #[test]
    fn builder_reproduces_file_window_menu() {
        let built = Menu::builder()
            .submenu("~F~ile", alt('f'), |m| {
                m.command_key("~O~pen", Command::OPEN, KeyEvent::from(Key::F(3)), "F3")
                    .command("~N~ew", Command::NEW)
                    .separator()
                    .command_key("E~x~it", Command::QUIT, alt('x'), "Alt-X")
            })
            .submenu("~W~indow", alt('w'), |m| {
                m.command_key("~N~ext", Command::NEXT, KeyEvent::from(Key::F(6)), "")
            })
            .build();

        let expected = Menu {
            items: vec![
                MenuItem::SubMenu {
                    name: "~F~ile".to_string(),
                    key_code: Some(alt('f')),
                    help_ctx: HelpCtx::NO_CONTEXT,
                    disabled: false,
                    menu: Menu {
                        items: vec![
                            MenuItem::Command {
                                name: "~O~pen".to_string(),
                                command: Command::OPEN,
                                key_code: Some(KeyEvent::from(Key::F(3))),
                                param: Some("F3".to_string()),
                                help_ctx: HelpCtx::NO_CONTEXT,
                                disabled: false,
                            },
                            MenuItem::Command {
                                name: "~N~ew".to_string(),
                                command: Command::NEW,
                                key_code: None,
                                param: None,
                                help_ctx: HelpCtx::NO_CONTEXT,
                                disabled: false,
                            },
                            MenuItem::Separator,
                            MenuItem::Command {
                                name: "E~x~it".to_string(),
                                command: Command::QUIT,
                                key_code: Some(alt('x')),
                                param: Some("Alt-X".to_string()),
                                help_ctx: HelpCtx::NO_CONTEXT,
                                disabled: false,
                            },
                        ],
                        default: Some(0),
                    },
                },
                MenuItem::SubMenu {
                    name: "~W~indow".to_string(),
                    key_code: Some(alt('w')),
                    help_ctx: HelpCtx::NO_CONTEXT,
                    disabled: false,
                    menu: Menu {
                        items: vec![MenuItem::Command {
                            name: "~N~ext".to_string(),
                            command: Command::NEXT,
                            key_code: Some(KeyEvent::from(Key::F(6))),
                            param: None, // empty "" param → None (C++ param == 0)
                            help_ctx: HelpCtx::NO_CONTEXT,
                            disabled: false,
                        }],
                        default: Some(0),
                    },
                },
            ],
            default: Some(0),
        };

        assert_eq!(built, expected);
    }

    #[test]
    fn empty_builder_has_no_items_and_no_default() {
        let menu = Menu::builder().build();
        assert!(menu.items.is_empty());
        assert_eq!(menu.default, None);
    }

    #[test]
    fn command_without_accelerator_has_no_key_code() {
        let menu = Menu::builder().command("~N~ew", Command::NEW).build();
        assert_eq!(menu.default, Some(0));
        match &menu.items[0] {
            MenuItem::Command {
                command,
                key_code,
                param,
                ..
            } => {
                assert_eq!(*command, Command::NEW);
                assert_eq!(*key_code, None);
                assert_eq!(*param, None);
            }
            other => panic!("expected a Command item, got {other:?}"),
        }
    }

    #[test]
    fn empty_param_string_is_stored_as_none() {
        let menu = Menu::builder()
            .command_key("~N~ext", Command::NEXT, KeyEvent::from(Key::F(6)), "")
            .build();
        match &menu.items[0] {
            MenuItem::Command {
                param, key_code, ..
            } => {
                assert_eq!(*param, None);
                // The accelerator must still be present — not swallowed.
                assert_eq!(*key_code, Some(KeyEvent::from(Key::F(6))));
            }
            other => panic!("expected a Command item, got {other:?}"),
        }
    }

    #[test]
    fn raw_item_escape_hatch_sets_default_and_carries_full_control() {
        let menu = Menu::builder()
            .item(MenuItem::Command {
                name: "~D~isabled".to_string(),
                command: Command::SAVE,
                key_code: None,
                param: None,
                help_ctx: HelpCtx::custom("myapp.save"),
                disabled: true,
            })
            .build();
        assert_eq!(menu.default, Some(0));
        match &menu.items[0] {
            MenuItem::Command {
                help_ctx, disabled, ..
            } => {
                assert_eq!(*help_ctx, HelpCtx::custom("myapp.save"));
                assert!(*disabled);
            }
            other => panic!("expected a Command item, got {other:?}"),
        }
    }

    #[test]
    fn disabled_mut_is_none_for_separator_and_live_otherwise() {
        let mut sep = MenuItem::Separator;
        assert_eq!(sep.disabled_mut(), None);

        let mut cmd = MenuItem::Command {
            name: "~X~".to_string(),
            command: Command::QUIT,
            key_code: None,
            param: None,
            help_ctx: HelpCtx::NO_CONTEXT,
            disabled: false,
        };
        *cmd.disabled_mut().expect("command has a disabled flag") = true;
        match &cmd {
            MenuItem::Command { disabled, .. } => assert!(*disabled),
            _ => unreachable!(),
        }

        let mut sub = MenuItem::SubMenu {
            name: "~F~ile".to_string(),
            key_code: None,
            help_ctx: HelpCtx::NO_CONTEXT,
            disabled: false,
            menu: Menu::default(),
        };
        *sub.disabled_mut().expect("submenu has a disabled flag") = true;
        match &sub {
            MenuItem::SubMenu { disabled, .. } => assert!(*disabled),
            _ => unreachable!(),
        }
    }
}
