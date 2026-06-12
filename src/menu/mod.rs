//! Menus: the data tree and the views that present it.
//!
//! [`MenuItem`]/[`Menu`]/[`MenuBuilder`] are the pure data — define a menu tree
//! fluently with the builder. [`MenuBar`], [`MenuBox`], and the menu session (see
//! [`menu_view`], [`menu_bar`], [`menu_box`], [`menu_session`]) draw it and run the
//! interactive open/navigate/select loop.
//!
//! ## The item model
//!
//! A [`MenuItem`] is one of three things, made explicit and type-safe by the
//! enum's three variants:
//!
//! - a **separator** — a horizontal divider with no label or behaviour;
//! - a **submenu** — a label that opens a nested [`Menu`];
//! - a **command item** — a label that emits a [`Command`] when chosen, with an
//!   optional accelerator key and optional shortcut display text such as
//!   `"Alt-X"`.
//!
//! A command and a submenu are mutually exclusive: an item can never hold both a
//! command and a nested menu. The fields common to commands and submenus
//! (`name`, `key_code`, `help_ctx`, `disabled`) are read uniformly via
//! or-patterns, e.g. `Command { disabled, .. } | SubMenu { disabled, .. } => …`.
//!
//! A [`Menu`] is a [`Vec`] of items plus a `default` selection given as an
//! *index* into that vector (any valid index, or `None`). The [`MenuBuilder`]
//! produces the usual convention: `default` is `Some(0)` (the first item) for a
//! non-empty menu and `None` for an empty one.
//!
//! **Guide:** [Menus, status line & help](../../../apps/menus.html).
//!
//! # Turbo Vision heritage
//! Ports `TMenuItem`, `TSubMenu`, and `TMenu` (`menus.h`/`menu.cpp`). The
//! implicit `name`/`command`-tagged union becomes a 3-variant enum and the
//! linked list becomes a `Vec` with an index default (deviation D1).

use crate::command::Command;
use crate::event::{Key, KeyEvent, KeyModifiers};
use crate::help::HelpCtx;

pub mod menu_bar;
pub mod menu_box;
pub mod menu_session;
pub mod menu_view;
pub use menu_bar::MenuBar;
pub use menu_box::MenuBox;
pub use menu_session::{MenuSession, popup_menu};
pub use menu_view::{MenuColors, MenuView, MenuViewState};

/// A single menu entry: a [`Separator`], a [`Command`] item, or a [`SubMenu`].
///
/// A command item carries the label, the [`Command`] it emits, and its optional
/// accelerator key and shortcut display text. A submenu carries a label and an
/// owned nested [`Menu`]. A separator is a divider with no other data. The
/// command-vs-submenu split is exclusive: an entry holds either a command or a
/// nested menu, never both.
///
/// [`Separator`]: MenuItem::Separator
/// [`Command`]: MenuItem::Command
/// [`SubMenu`]: MenuItem::SubMenu
///
/// # Turbo Vision heritage
/// Ports `TMenuItem` (`menus.h`), folding in its `TSubMenu` subclass and the
/// `newLine()` separator. The implicit `union { const char *param; TMenu
/// *subMenu; }` (tagged by `name`/`command`) becomes the three explicit variants
/// (deviation D1).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum MenuItem {
    /// A horizontal divider line with no label or behaviour.
    Separator,

    /// A command item: a label that emits a [`Command`] when chosen.
    Command {
        /// The label; a `~`-marked character is the highlighted hotkey.
        name: String,
        /// The command emitted when this item is chosen.
        command: Command,
        /// The accelerator key that activates this item from anywhere. `None`
        /// means the item has no accelerator.
        key_code: Option<KeyEvent>,
        /// Shortcut display text such as `"Alt-X"`, shown right-aligned. `None`
        /// means no shortcut text.
        param: Option<String>,
        /// The help context active while this item is highlighted.
        help_ctx: HelpCtx,
        /// Whether the item is greyed out. Mutated at runtime by command-graying,
        /// which keeps it in sync with the live command set.
        disabled: bool,
    },

    /// A submenu: a label that opens a nested [`Menu`].
    SubMenu {
        /// The label; a `~`-marked character is the highlighted hotkey.
        name: String,
        /// The accelerator key that opens this submenu. See [`MenuItem::Command`].
        key_code: Option<KeyEvent>,
        /// The help context active while this item is highlighted.
        help_ctx: HelpCtx,
        /// Whether the item is greyed out.
        disabled: bool,
        /// The owned nested menu opened by this item.
        menu: Menu,
    },
}

impl MenuItem {
    /// A mutable handle to the `disabled` flag, or `None` for a
    /// [`Separator`](MenuItem::Separator) (which has no such field).
    ///
    /// Command-graying only ever toggles command and submenu items, never a
    /// separator, so a separator yields `None`.
    pub fn disabled_mut(&mut self) -> Option<&mut bool> {
        match self {
            MenuItem::Separator => None,
            MenuItem::Command { disabled, .. } | MenuItem::SubMenu { disabled, .. } => {
                Some(disabled)
            }
        }
    }
}

/// A menu: an ordered list of [`MenuItem`]s plus the default selection.
///
/// The default is an *index* into [`items`](field@Menu::items) — any valid
/// index, or `None` for no default. Both fields are public and not
/// invariant-checked; the [`MenuBuilder`] is what produces the common
/// `Some(0)`-first / `None`-empty convention (see its docs).
///
/// # Turbo Vision heritage
/// Ports `TMenu` (`menus.h`): the linked list of items becomes a [`Vec`] and the
/// `deflt` pointer becomes an index into it (deviation D1).
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Menu {
    /// The menu entries, in order.
    pub items: Vec<MenuItem>,
    /// The index of the default item; any valid index into [`items`], or `None`
    /// for no default. The [`MenuBuilder`] sets this to `Some(0)` (the first
    /// item) for a non-empty menu and `None` for an empty one, but the field is
    /// unconstrained.
    ///
    /// [`items`]: Menu::items
    pub default: Option<usize>,
}

impl Menu {
    /// Start building a menu fluently.
    pub fn builder() -> MenuBuilder {
        MenuBuilder::default()
    }
}

/// A fluent builder for a [`Menu`].
///
/// Each method appends one [`MenuItem`] and returns `self` for chaining; nested
/// submenus are built with a closure. The first appended item sets
/// [`Menu::default`] to `Some(0)`.
///
/// # Turbo Vision heritage
/// Replaces the `operator+` overloads (`menu.cpp`) that chained `TSubMenu` and
/// `TMenuItem` nodes together.
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

    /// Append a separator line.
    pub fn separator(self) -> Self {
        self.item(MenuItem::Separator)
    }

    /// Append a command item with no accelerator and no shortcut text.
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

    /// Append a command item with an accelerator and shortcut display text.
    ///
    /// An empty `param` string is stored as `None` (no shortcut text).
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

    /// Append a submenu whose contents are built by the closure.
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
/// keeps menu trees terse at the call site.
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

    /// The builder must reproduce, node for node, the canonical File/Window menu
    /// tree. The expected tree is hand-built with struct/enum literals (a
    /// *different* code path from the builder) so a builder bug cannot pass
    /// silently.
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
