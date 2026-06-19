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
/// Use [`MenuBuilder`] for the common case: `.command()`, `.command_key()`,
/// `.submenu()`, and `.separator()` cover the usual patterns. Construct a
/// `MenuItem` literal directly when you need fields the builder does not expose
/// (e.g. a custom `help_ctx` or a pre-disabled item), then pass it via
/// [`MenuBuilder::item`].
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
    ///
    /// Append one with [`MenuBuilder::separator`].
    Separator,

    /// A command item: a label that emits a [`Command`] when chosen.
    ///
    /// When the user selects this item (via keyboard or mouse) the menu session
    /// closes and posts the [`command`](MenuItem::Command::command) as a
    /// [`Event::Command`] broadcast. Use [`MenuBuilder::command`] for items with
    /// no accelerator, or [`MenuBuilder::command_key`] for items with an
    /// accelerator key and shortcut display text.
    ///
    /// [`Event::Command`]: crate::event::Event::Command
    Command {
        /// The menu item label. Wrap one character in tildes (`~X~`) to mark it
        /// as the single-key hotkey — the menu session will highlight that
        /// character and let the user press it alone to select the item.
        name: String,
        /// The [`Command`] broadcast when this item is chosen.
        ///
        /// The menu session posts this as an [`Event::Command`] broadcast after
        /// closing the menu. The command must be *enabled* in the application's
        /// command set for the item to be selectable; the menu session
        /// automatically greys it out (via [`disabled`](MenuItem::Command::disabled))
        /// when the command is disabled.
        ///
        /// [`Event::Command`]: crate::event::Event::Command
        command: Command,
        /// The global accelerator key for this item. `None` means no accelerator.
        ///
        /// When set, the user can press this key combination from anywhere in
        /// the application (not just inside an open menu) to trigger the item.
        /// Use [`alt()`] to build an `Alt`+`<char>` key, or construct a
        /// [`KeyEvent`] directly for other combinations.
        key_code: Option<KeyEvent>,
        /// Right-aligned shortcut hint shown next to the item label, e.g.
        /// `"Alt-X"`. `None` means no hint is shown. Use `None` (or an empty
        /// string via the builder) when the item has no accelerator, or when you
        /// don't want to advertise it in the menu.
        ///
        /// This is display text only — the [`key_code`](MenuItem::Command::key_code)
        /// field is the actual binding.
        param: Option<String>,
        /// The help context shown when this item is highlighted.
        ///
        /// Set to [`HelpCtx::NO_CONTEXT`] when no context-sensitive help is
        /// needed. Pass a custom value only if your application ships a help
        /// system keyed on context ids.
        help_ctx: HelpCtx,
        /// Whether this item is greyed out and unselectable.
        ///
        /// For a command item this flag is **framework-managed**: each time the
        /// menu opens, the regray pass (`update_menu_commands`) overwrites it with
        /// whether `command` is in the program's live disabled-command set
        /// (`disabled = disabled_cmds.has(command)`). Setting it at menu-build time
        /// therefore has no lasting effect for a command item — disable the command
        /// itself instead (e.g. via a [`Deferred`] command-set effect). The public
        /// [`MenuItem::disabled_mut`] accessor exists for custom graying logic; the
        /// regray pass writes the field directly rather than through it.
        ///
        /// [`Deferred`]: crate::view::Deferred
        disabled: bool,
    },

    /// A submenu: a label that opens a nested [`Menu`] in a pop-up box.
    ///
    /// When the user navigates to this item the menu session opens [`menu`]
    /// as a child menu box. Use [`MenuBuilder::submenu`] to build the nested
    /// menu with a closure; construct this variant directly only when you need to
    /// set a custom `help_ctx` (the builder always uses [`HelpCtx::NO_CONTEXT`]).
    ///
    /// [`menu`]: MenuItem::SubMenu::menu
    SubMenu {
        /// The submenu label. Wrap one character in tildes (`~X~`) to mark the
        /// single-key hotkey, the same as for [`MenuItem::Command::name`].
        name: String,
        /// The global accelerator key for this submenu. `None` means no accelerator.
        ///
        /// When set the user can press this key to open the submenu from anywhere.
        /// See [`MenuItem::Command::key_code`] and [`alt()`] for construction.
        key_code: Option<KeyEvent>,
        /// The help context shown while this submenu label is highlighted.
        ///
        /// The builder always sets this to [`HelpCtx::NO_CONTEXT`]. To attach a
        /// non-default context, construct the variant directly and pass it to
        /// [`MenuBuilder::item`].
        help_ctx: HelpCtx,
        /// Whether this submenu label is greyed out. Unlike
        /// [`MenuItem::Command::disabled`], the regray pass does **not** manage a
        /// submenu's own flag — set it at build time (or via
        /// [`disabled_mut`](MenuItem::disabled_mut)) to grey a whole submenu.
        disabled: bool,
        /// The nested [`Menu`] opened when this item is selected.
        ///
        /// The [`Menu`] is owned — the `SubMenu` variant holds the full tree, with
        /// no raw pointer or manual disposal. Build the nested menu with
        /// [`Menu::builder()`] or pass a pre-built [`Menu`] value directly.
        menu: Menu,
    },
}

impl MenuItem {
    /// Returns a mutable reference to the `disabled` flag, or `None` for a
    /// [`Separator`](MenuItem::Separator), which has no disabled state.
    ///
    /// Use this for custom graying logic: call `disabled_mut()`, check for `Some`,
    /// and set the flag. Note the framework's own regray pass
    /// (`update_menu_commands`) writes a command item's flag directly from the
    /// program's disabled-command set rather than through this accessor, so for
    /// command items prefer disabling the command itself; this handle is most
    /// useful for greying a whole submenu. Separators are never greyable so they
    /// yield `None`.
    pub fn disabled_mut(&mut self) -> Option<&mut bool> {
        match self {
            MenuItem::Separator => None,
            MenuItem::Command { disabled, .. } | MenuItem::SubMenu { disabled, .. } => {
                Some(disabled)
            }
        }
    }
}

/// An ordered list of [`MenuItem`]s with an optional default selection.
///
/// A `Menu` is the data backing a drop-down or pop-up menu: a flat list of
/// entries plus the index of the item that is pre-selected when the menu opens.
/// Use [`Menu::builder()`] to construct one fluently; the builder sets
/// [`default`](Menu::default) automatically. Construct a `Menu` literal only
/// when you need precise control over the default index — for example, when you
/// want to pre-select an item other than the first:
///
/// ```rust
/// use tvision_rs::menu::{Menu, MenuItem};
/// use tvision_rs::command::Command;
/// use tvision_rs::help::HelpCtx;
///
/// // Pre-built items.
/// let items = vec![
///     MenuItem::Command {
///         name: "~N~ew".into(), command: Command::NEW,
///         key_code: None, param: None,
///         help_ctx: HelpCtx::NO_CONTEXT, disabled: false,
///     },
///     MenuItem::Command {
///         name: "~O~pen".into(), command: Command::OPEN,
///         key_code: None, param: None,
///         help_ctx: HelpCtx::NO_CONTEXT, disabled: false,
///     },
/// ];
/// // Pre-select "Open" (index 1) rather than "New" (index 0).
/// let menu = Menu { items, default: Some(1) };
/// assert_eq!(menu.default, Some(1));
/// ```
///
/// Both fields are public and not invariant-checked: `default` may be any valid
/// index into `items`, or `None`. [`Default::default()`] gives an empty menu
/// with no pre-selection.
///
/// # Turbo Vision heritage
/// Ports `TMenu` (`menus.h`): the singly-linked list of items becomes a [`Vec`]
/// and the `deflt` pointer becomes an index into it.
#[derive(Clone, Debug, PartialEq, Eq, Default)]
pub struct Menu {
    /// The menu entries, in display order.
    ///
    /// Iterate with `.iter()` / `.iter_mut()` or index directly. Modify in
    /// place (e.g. to disable a specific item by index) only between menu
    /// activations: while a [`MenuSession`] is open it holds a **clone** of the
    /// menu, so changes to this vec are not visible until the next activation.
    ///
    /// [`MenuSession`]: crate::menu::MenuSession
    pub items: Vec<MenuItem>,
    /// The index of the item that is highlighted when the menu first opens.
    ///
    /// Any valid index into [`items`], or `None` for no initial highlight.
    /// [`MenuBuilder`] sets this to `Some(0)` for a non-empty menu and `None`
    /// for an empty one. To pre-select a different item, either set this field
    /// directly after building (`menu.default = Some(n)`) or construct the
    /// struct literal. The session clones the whole `Menu` at open and reads
    /// this field once as the initial highlight; subsequent navigation does not
    /// write it back to the original, so re-activating the menu always restarts
    /// on this index.
    ///
    /// [`items`]: Menu::items
    pub default: Option<usize>,
}

impl Menu {
    /// Start building a menu with the fluent [`MenuBuilder`].
    ///
    /// The returned builder starts empty; chain `.command()`, `.command_key()`,
    /// `.submenu()`, and `.separator()` calls, then finish with `.build()`.
    /// The first item appended automatically becomes the default selection
    /// (`default = Some(0)`). Use this for the common case; if you need a
    /// non-first default or full control over every field, construct a `Menu`
    /// struct literal directly (both fields are `pub`).
    pub fn builder() -> MenuBuilder {
        MenuBuilder::default()
    }
}

/// A fluent builder for a [`Menu`].
///
/// Start a builder via [`Menu::builder()`] or `MenuBuilder::default()`, chain
/// item-appending methods, and finish with [`.build()`](MenuBuilder::build):
///
/// ```rust
/// use tvision_rs::menu::{Menu, alt};
/// use tvision_rs::command::Command;
///
/// let menu = Menu::builder()
///     .submenu("~F~ile", alt('f'), |m| {
///         m.command_key("E~x~it", Command::QUIT, alt('x'), "Alt-X")
///     })
///     .build();
/// ```
///
/// Each method appends exactly one [`MenuItem`] and returns `self` for
/// chaining. The first item appended sets [`Menu::default`] to `Some(0)`,
/// making the first item pre-selected when the menu opens. **The builder
/// always uses `Some(0)` as the default** — there is no builder method to
/// choose a different pre-selected item. If you need a non-first default
/// (e.g. to match the Turbo Vision two-argument `TMenu(itemList, theDefault)`
/// constructor), build the item list with the builder, then override the field:
/// `let mut menu = builder.build(); menu.default = Some(n);`, or construct a
/// [`Menu`] struct literal directly (both fields are `pub`).
///
/// For items that need full field control (custom `help_ctx`, pre-set
/// `disabled`, etc.) use [`MenuBuilder::item`] with a [`MenuItem`] literal.
///
/// # Turbo Vision heritage
/// Replaces the `operator+` overloads (`menu.cpp`) that chained `TSubMenu` and
/// `TMenuItem` nodes together, and the `NewItem`/`NewLine`/`NewSubMenu`
/// constructor functions from the Pascal-era API.
#[derive(Default)]
pub struct MenuBuilder {
    menu: Menu,
}

impl MenuBuilder {
    /// Append an already-built [`MenuItem`], giving full control over every field.
    ///
    /// Use this when the convenience methods ([`command`](MenuBuilder::command),
    /// [`command_key`](MenuBuilder::command_key), [`submenu`](MenuBuilder::submenu))
    /// don't expose what you need — for example, a custom `help_ctx`, a
    /// pre-disabled item, or a submenu with a non-`NO_CONTEXT` help id:
    ///
    /// ```rust
    /// use tvision_rs::menu::{Menu, MenuItem};
    /// use tvision_rs::command::Command;
    /// use tvision_rs::help::HelpCtx;
    ///
    /// let menu = Menu::builder()
    ///     .item(MenuItem::Command {
    ///         name: "~S~ave".into(),
    ///         command: Command::SAVE,
    ///         key_code: None,
    ///         param: None,
    ///         help_ctx: HelpCtx::custom("myapp.save"),
    ///         disabled: false,
    ///     })
    ///     .build();
    /// ```
    pub fn item(mut self, item: MenuItem) -> Self {
        self.menu.items.push(item);
        if self.menu.default.is_none() {
            self.menu.default = Some(0);
        }
        self
    }

    /// Append a [`MenuItem::Separator`] — a horizontal divider line.
    ///
    /// Corresponds to the classic Turbo Vision `NewLine()` helper (`newLine()` in
    /// magiblot C++).
    pub fn separator(self) -> Self {
        self.item(MenuItem::Separator)
    }

    /// Append a [`MenuItem::Command`] with no accelerator and no shortcut text.
    ///
    /// Use this for items that are triggered only by navigating with arrow keys
    /// or by their hotkey letter, not by a global accelerator. For items with an
    /// accelerator use [`command_key`](MenuBuilder::command_key) instead.
    ///
    /// All convenience builder methods use [`HelpCtx::NO_CONTEXT`] and
    /// `disabled: false`. To set a custom `help_ctx`, pre-disable the item, or
    /// control any other non-default field, build the item explicitly with
    /// [`MenuBuilder::item`] and a [`MenuItem::Command`] literal.
    ///
    /// Corresponds to `NewItem(name, "", 0, cmd, hcNoContext, 0)` in the classic
    /// (Pascal-era) Turbo Vision API; magiblot C++ builds the equivalent
    /// `TMenuItem` via `operator+`.
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

    /// Append a [`MenuItem::Command`] with an accelerator key and shortcut display text.
    ///
    /// `key_code` is the global accelerator (use [`alt()`] for `Alt`+`<char>`
    /// combinations, or pass a [`KeyEvent`] directly for others). `param` is the
    /// display hint shown right-aligned next to the label (e.g. `"Alt-X"`,
    /// `"F3"`); an empty string is stored as `None` (no hint shown).
    ///
    /// ```rust
    /// use tvision_rs::menu::{Menu, alt};
    /// use tvision_rs::command::Command;
    /// use tvision_rs::event::{Key, KeyEvent};
    ///
    /// let menu = Menu::builder()
    ///     .command_key("~O~pen", Command::OPEN, KeyEvent::from(Key::F(3)), "F3")
    ///     .command_key("E~x~it", Command::QUIT, alt('x'), "Alt-X")
    ///     .build();
    /// ```
    ///
    /// See [`MenuBuilder::command`] for the `HelpCtx`/`disabled` escape hatch.
    ///
    /// Corresponds to `NewItem(name, param, key, cmd, hcNoContext, 0)` in the
    /// classic (Pascal-era) Turbo Vision API.
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

    /// Append a [`MenuItem::SubMenu`] whose contents are built by the closure.
    ///
    /// `name` is the label (tilde-hotkey syntax applies). `key_code` is the
    /// optional global accelerator (use [`alt()`] or `None`). The closure
    /// receives a fresh [`MenuBuilder`] for the nested menu and must return it
    /// after appending items:
    ///
    /// ```rust
    /// use tvision_rs::menu::{Menu, alt};
    /// use tvision_rs::command::Command;
    ///
    /// let menu = Menu::builder()
    ///     .submenu("~F~ile", alt('f'), |m| {
    ///         m.command("~N~ew", Command::NEW)
    ///          .command("~O~pen", Command::OPEN)
    ///     })
    ///     .build();
    /// ```
    ///
    /// **`help_ctx` limitation:** this method always uses [`HelpCtx::NO_CONTEXT`]
    /// for the submenu item itself. To attach a custom `help_ctx`, construct the
    /// [`MenuItem::SubMenu`] variant directly and pass it to [`MenuBuilder::item`].
    ///
    /// Corresponds to `NewSubMenu(name, key, subMenu, hcNoContext, 0)` in the
    /// classic (Pascal-era) Turbo Vision API.
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

    /// Finish building and return the completed [`Menu`].
    pub fn build(self) -> Menu {
        self.menu
    }
}

/// Build an `Alt`+`<char>` [`KeyEvent`] for use as a menu accelerator.
///
/// This is the most common accelerator form for menu items. Pass the result
/// to [`MenuBuilder::command_key`] or [`MenuBuilder::submenu`], or store it
/// in [`MenuItem::Command::key_code`] / [`MenuItem::SubMenu::key_code`]:
///
/// ```rust
/// use tvision_rs::menu::{Menu, alt};
/// use tvision_rs::command::Command;
///
/// let menu = Menu::builder()
///     .submenu("~F~ile", alt('f'), |m| {
///         m.command_key("E~x~it", Command::QUIT, alt('x'), "Alt-X")
///     })
///     .build();
/// ```
///
/// For other key combinations (e.g. function keys) construct a [`KeyEvent`]
/// directly using [`KeyEvent::from(Key::F(n))`](KeyEvent::from).
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
