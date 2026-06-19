//! The status-line **data types** — [`StatusItem`], [`StatusDef`] — plus a
//! fluent builder. The [`StatusLine`](status_line::StatusLine) view (drawing,
//! mouse dispatch, hotkey accelerators, command graying) lives in
//! [`status_line`].
//!
//! A [`StatusItem`] is one entry: a label, an optional accelerator key, and the
//! command it emits. A [`StatusDef`] groups the items shown for a particular
//! set of help contexts ([`HelpCtxRange`]); the status line walks its defs and
//! shows the first one whose range matches the current help context.
//!
//! ### Hidden hotkey bindings
//!
//! A [`StatusItem`] with no text ([`StatusItem::text`] `== None`) displays
//! **nothing** and **consumes no horizontal space**, but its accelerator still
//! fires its command — an invisible global hotkey (e.g. Shift-Del → Cut). Draw
//! and mouse-hit-test skip such items; the keyboard accelerator does not.
//!
//! ### Which help contexts a def applies to
//!
//! A [`StatusDef`] selects its items based on the current help context. Because
//! a [`HelpCtx`] here is a namespaced `&'static str` with no ordering,
//! [`HelpCtxRange`] is a small matcher rather than a numeric range: the universal
//! def is [`HelpCtxRange::All`] (matches anything), and the rare context-split
//! case is an explicit membership set ([`HelpCtxRange::OneOf`]). It stays
//! `Clone + PartialEq + Eq`, like [`Menu`](crate::menu::Menu).
//!
//! **Guide:** [Menus, status line & help](../../../apps/menus.html).
//!
//! # Turbo Vision heritage
//!
//! Ports `TStatusItem` and `TStatusDef` (`menus.h`, `tstatusl.cpp`). The singly
//! linked lists become [`Vec`]s, and the numeric help-context range becomes
//! [`HelpCtxRange`] because help contexts are now namespaced strings rather than
//! integers (deviation D1).

use crate::command::Command;
use crate::event::KeyEvent;
use crate::help::HelpCtx;

pub mod status_line;
pub use status_line::{StatusColors, StatusLine};

/// A single status-line entry: a label, an optional accelerator, and a command.
///
/// `text == None` is a **hidden global hotkey binding** that draws nothing and
/// consumes no width but still fires its [`command`](StatusItem::command) when
/// its [`key_code`](StatusItem::key_code) is pressed (see the module docs).
///
/// # Turbo Vision heritage
///
/// Ports `TStatusItem` (`menus.h`); the optional label becomes `Option<String>`
/// (`None` = a hidden binding).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusItem {
    /// The displayed label. `None` is a hidden hotkey binding that draws nothing
    /// and consumes no horizontal space.
    pub text: Option<String>,
    /// The accelerator key that fires this item's command; `None` means the item
    /// has no keyboard shortcut and can only be activated by clicking it.
    ///
    /// Use [`StatusItem::key`] to build a *hidden* item whose only purpose is
    /// the accelerator (no label, no width). The C++ equivalent used a
    /// `kbNoKey` sentinel; `None` here is cleaner and carries the same meaning.
    pub key_code: Option<KeyEvent>,
    /// The command posted when this item is clicked or its accelerator is pressed.
    ///
    /// Typically one of the standard [`Command`] constants (e.g. `Command::HELP`,
    /// `Command::QUIT`), but any application-defined command works. The status
    /// line checks whether the command is currently enabled before posting it and
    /// before highlighting the item during a mouse drag.
    pub command: Command,
}

impl StatusItem {
    /// Build a visible item (`text`, optional accelerator, command).
    pub fn new(
        text: impl Into<String>,
        key_code: impl Into<Option<KeyEvent>>,
        command: Command,
    ) -> Self {
        StatusItem {
            text: Some(text.into()),
            key_code: key_code.into(),
            command,
        }
    }

    /// Build a hidden hotkey binding: no text, so it draws nothing and consumes
    /// no width, but its accelerator still fires `command`.
    pub fn key(key_code: impl Into<Option<KeyEvent>>, command: Command) -> Self {
        StatusItem {
            text: None,
            key_code: key_code.into(),
            command,
        }
    }
}

/// Which help contexts a [`StatusDef`] applies to (see the module docs).
#[derive(Clone, Debug, PartialEq, Eq)]
pub enum HelpCtxRange {
    /// The universal def every real app uses (the program-wide default). Matches
    /// **any** help context.
    All,
    /// The rare context-split case: an explicit set of help contexts this def
    /// applies to.
    OneOf(Vec<HelpCtx>),
}

impl HelpCtxRange {
    /// Whether `ctx` selects this def: `All` matches anything, `OneOf` matches
    /// membership in its set.
    pub fn matches(&self, ctx: HelpCtx) -> bool {
        match self {
            HelpCtxRange::All => true,
            HelpCtxRange::OneOf(set) => set.contains(&ctx),
        }
    }
}

/// A status-line definition: the items shown for a [`range`](StatusDef::range) of
/// help contexts.
///
/// # Turbo Vision heritage
///
/// Ports `TStatusDef` (`menus.h`); the linked list of defs becomes the outer
/// `Vec<StatusDef>` the [`StatusLine`](status_line::StatusLine) owns, and the
/// inner items list is a [`Vec<StatusItem>`].
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct StatusDef {
    /// Which help contexts activate this def.
    ///
    /// [`HelpCtxRange::All`] is the universal fallback used by nearly every app;
    /// [`HelpCtxRange::OneOf`] restricts the def to an explicit set of contexts.
    /// Place more-specific defs *before* the `All` def in the list — the status
    /// line picks the *first* match.
    ///
    /// # Turbo Vision heritage
    ///
    /// Replaces the `TStatusDef::min`/`max` integer pair (`ushort min, max`). Because
    /// [`HelpCtx`] is now a namespaced `&'static str` rather than an integer, a
    /// numeric range has no meaning; [`HelpCtxRange`] is the typed replacement.
    pub range: HelpCtxRange,
    /// The items displayed by the status line when this def is active.
    ///
    /// Build this list with [`StatusItemsBuilder`] via [`StatusDef::list`], or
    /// construct `StatusItem` values directly and collect them into a `Vec`. Items
    /// appear left-to-right on screen in `Vec` order; a `text == None` item is
    /// invisible but still contributes its accelerator binding.
    pub items: Vec<StatusItem>,
}

impl StatusDef {
    /// Start a fluent builder chain for the `Vec<StatusDef>` that
    /// [`StatusLine::new`](status_line::StatusLine::new) expects.
    ///
    /// Call [`def_all`](StatusDefListBuilder::def_all) (the universal
    /// context-independent bar used by most apps) or
    /// [`def_one_of`](StatusDefListBuilder::def_one_of) (context-specific
    /// bar) to append defs, then finish with
    /// [`build`](StatusDefListBuilder::build). The status line walks the
    /// list in order and activates the first def whose
    /// [`range`](StatusDef::range) matches the current help context, so place
    /// context-specific defs *before* the `All` def.
    ///
    /// # Example
    ///
    /// ```
    /// use tvision_rs::status::{StatusDef, StatusLine};
    /// use tvision_rs::command::Command;
    /// use tvision_rs::event::{Key, KeyEvent};
    ///
    /// let defs = StatusDef::list()
    ///     .def_all(|d| {
    ///         d.item("~F1~ Help", KeyEvent::from(Key::F(1)), Command::HELP)
    ///          .item("~Alt-X~ Exit", tvision_rs::menu::alt('x'), Command::QUIT)
    ///          .key_item(KeyEvent::from(Key::F(10)), Command::MENU)
    ///     })
    ///     .build();
    ///
    /// let _status_line = StatusLine::new(
    ///     tvision_rs::view::Rect::new(0, 24, 80, 25),
    ///     defs,
    /// );
    /// ```
    pub fn list() -> StatusDefListBuilder {
        StatusDefListBuilder::default()
    }
}

/// A fluent builder for the `Vec<StatusDef>` a [`StatusLine`](status_line::StatusLine) owns.
///
/// Start a chain with [`StatusDef::list`], append defs with
/// [`def_all`](Self::def_all) (the universal fallback) or
/// [`def_one_of`](Self::def_one_of) (context-restricted), and finish with
/// [`build`](Self::build). The resulting `Vec<StatusDef>` is passed directly to
/// [`StatusLine::new`](status_line::StatusLine::new).
///
/// The status line walks the defs in order and activates the first one whose
/// [`range`](StatusDef::range) matches the current help context, so put
/// context-specific defs before the universal `All` def.
#[derive(Default)]
pub struct StatusDefListBuilder {
    defs: Vec<StatusDef>,
}

impl StatusDefListBuilder {
    /// Append the universal def ([`HelpCtxRange::All`], matches any context). The
    /// closure fills its items.
    pub fn def_all(mut self, build: impl FnOnce(StatusItemsBuilder) -> StatusItemsBuilder) -> Self {
        let items = build(StatusItemsBuilder::default()).build();
        self.defs.push(StatusDef {
            range: HelpCtxRange::All,
            items,
        });
        self
    }

    /// Append a context-restricted def ([`HelpCtxRange::OneOf`]) — the rare case
    /// where the status line differs per help context. The closure fills its
    /// items.
    pub fn def_one_of(
        mut self,
        contexts: impl IntoIterator<Item = HelpCtx>,
        build: impl FnOnce(StatusItemsBuilder) -> StatusItemsBuilder,
    ) -> Self {
        let items = build(StatusItemsBuilder::default()).build();
        self.defs.push(StatusDef {
            range: HelpCtxRange::OneOf(contexts.into_iter().collect()),
            items,
        });
        self
    }

    /// Append an already-built [`StatusDef`] — the escape hatch.
    pub fn def(mut self, def: StatusDef) -> Self {
        self.defs.push(def);
        self
    }

    /// Finish and produce the `Vec<StatusDef>`.
    pub fn build(self) -> Vec<StatusDef> {
        self.defs
    }
}

/// A fluent builder for one def's `Vec<StatusItem>`.
///
/// Obtain one from the closure argument of [`StatusDefListBuilder::def_all`] or
/// [`StatusDefListBuilder::def_one_of`], then chain calls to [`item`](Self::item),
/// [`key_item`](Self::key_item), and [`raw`](Self::raw). The builder is consumed
/// by each call and returned, so the chain reads left-to-right in the order items
/// appear on screen. Call [`build`](Self::build) (or let the outer builder call it
/// for you) to produce the finished `Vec<StatusItem>`.
#[derive(Default)]
pub struct StatusItemsBuilder {
    items: Vec<StatusItem>,
}

impl StatusItemsBuilder {
    /// Append a visible item (label, optional accelerator, command).
    pub fn item(
        mut self,
        text: impl Into<String>,
        key_code: impl Into<Option<KeyEvent>>,
        command: Command,
    ) -> Self {
        self.items.push(StatusItem::new(text, key_code, command));
        self
    }

    /// Append a hidden hotkey binding (no text).
    pub fn key_item(mut self, key_code: impl Into<Option<KeyEvent>>, command: Command) -> Self {
        self.items.push(StatusItem::key(key_code, command));
        self
    }

    /// Append an already-built [`StatusItem`] — the escape hatch.
    pub fn raw(mut self, item: StatusItem) -> Self {
        self.items.push(item);
        self
    }

    /// Finish and produce the `Vec<StatusItem>`.
    pub fn build(self) -> Vec<StatusItem> {
        self.items
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::event::Key;
    use crate::menu::alt;

    fn f1() -> KeyEvent {
        KeyEvent::from(Key::F(1))
    }

    /// The builder must reproduce, node for node, the canonical default
    /// status-line tree. The expected tree is hand-built with struct/enum literals
    /// so a builder bug cannot pass silently.
    #[test]
    fn builder_reproduces_default_status_line() {
        let built = StatusDef::list()
            .def_all(|d| {
                d.item("~F1~ Help", f1(), Command::HELP)
                    .item("~Alt-X~ Exit", alt('x'), Command::QUIT)
                    .key_item(KeyEvent::from(Key::F(10)), Command::MENU)
            })
            .build();

        let expected = vec![StatusDef {
            range: HelpCtxRange::All,
            items: vec![
                StatusItem {
                    text: Some("~F1~ Help".to_string()),
                    key_code: Some(f1()),
                    command: Command::HELP,
                },
                StatusItem {
                    text: Some("~Alt-X~ Exit".to_string()),
                    key_code: Some(alt('x')),
                    command: Command::QUIT,
                },
                StatusItem {
                    text: None, // hidden hotkey binding (no label)
                    key_code: Some(KeyEvent::from(Key::F(10))),
                    command: Command::MENU,
                },
            ],
        }];

        assert_eq!(built, expected);
    }

    #[test]
    fn key_item_has_no_text() {
        let item = StatusItem::key(f1(), Command::HELP);
        assert_eq!(item.text, None);
        assert_eq!(item.key_code, Some(f1()));
        assert_eq!(item.command, Command::HELP);
    }

    #[test]
    fn item_without_accelerator_has_no_key_code() {
        let item = StatusItem::new("~F1~ Help", None, Command::HELP);
        assert_eq!(item.text.as_deref(), Some("~F1~ Help"));
        assert_eq!(item.key_code, None);
    }

    // -- HelpCtxRange::matches ----------------------------------------------

    #[test]
    fn range_all_matches_any_context() {
        assert!(HelpCtxRange::All.matches(HelpCtx::NO_CONTEXT));
        assert!(HelpCtxRange::All.matches(HelpCtx::custom("anything.at.all")));
    }

    #[test]
    fn range_one_of_matches_only_members() {
        let a = HelpCtx::custom("app.editor");
        let b = HelpCtx::custom("app.browser");
        let range = HelpCtxRange::OneOf(vec![a]);
        assert!(range.matches(a), "a is a member");
        assert!(!range.matches(b), "b is not a member");
    }

    #[test]
    fn def_one_of_collects_contexts() {
        let a = HelpCtx::custom("app.editor");
        let defs = StatusDef::list()
            .def_one_of([a], |d| d.item("~F2~ Save", None, Command::SAVE))
            .def_all(|d| d.item("~F1~ Help", f1(), Command::HELP))
            .build();
        assert_eq!(defs.len(), 2);
        assert_eq!(defs[0].range, HelpCtxRange::OneOf(vec![a]));
        assert_eq!(defs[1].range, HelpCtxRange::All);
    }
}
