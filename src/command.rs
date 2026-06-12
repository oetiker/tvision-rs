//! Commands and command sets.
//!
//! A [`Command`] is the identity of an action — the thing a menu item, button,
//! or key binding emits and a view interprets. Each command is an opaque
//! newtype around a **namespaced static string** (`"tv.ok"`), so application-
//! and view-defined commands are collision-safe by construction. A
//! [`CommandSet`] is a set of commands the framework enables or disables in
//! bulk.
//!
//! The associated constants below are **only the framework's shared
//! vocabulary** — commands the core (program/view/window/dialog/menu/
//! status-line/desktop) generates or interprets generically, all under the
//! `tv.` namespace. **View-specific commands live with their view module**
//! (e.g. the editor's `char_left`, the file dialog's `file_open`); external
//! views and apps mint their own the same way, via [`Command::custom`] with
//! their own dotted prefix.
//!
//! # Turbo Vision heritage
//!
//! Ports the `cm*` command family and `TCommandSet` (`views.h`,
//! `tcmdset.cpp`). C++ commands were hand-assigned `int`s; here a command's
//! identity is a namespaced `&'static str` instead, so the command space is
//! open and extensions cannot collide (deviation D1). The integers existed
//! only for serialization (dropped) and to index a 256-bit set (now a
//! [`HashSet`]), so commands no longer carry a number.

use std::collections::HashSet;
use std::ops::{AddAssign, BitAndAssign, BitOrAssign, SubAssign};

/// The identity of an action — what a menu item, button, or key binding emits
/// and a view interprets. A command's identity is a **namespaced static
/// string** rather than a number, so downstream code can mint
/// application-specific commands collision-safely.
///
/// The field is private: a `Command` is an opaque token. The associated
/// constants below are the framework's standard vocabulary (each annotated with
/// the C++ symbol it ports), and external apps/views use [`Command::custom`] to
/// define their own, namespacing under their own dotted prefix:
///
/// ```
/// const REFRESH: tvision::Command = tvision::Command::custom("myapp.refresh");
/// ```
///
/// Equality and hashing compare the string *contents*, so two `Command`s with
/// equal-content names are equal regardless of where the literals live.
///
/// [`Default`] is [`Command::VALID`] (the zero command).
///
/// # Turbo Vision heritage
///
/// Faithful to the `cm*` command family (`views.h`), which were plain `int`s;
/// here a command's identity is a namespaced `&'static str` (deviation D1).
#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub struct Command(&'static str);

impl Default for Command {
    fn default() -> Self {
        Command::VALID
    }
}

impl Command {
    /// Mint an application- or view-specific command from a namespaced name.
    ///
    /// The escape hatch external code uses to define its own commands; pick a
    /// dotted prefix unique to your app/view (`"myapp.refresh"`) so it cannot
    /// collide with the framework's `tv.*` vocabulary or another extension's.
    pub const fn custom(name: &'static str) -> Command {
        Command(name)
    }

    /// The command's namespaced name (e.g. `"tv.ok"`).
    pub const fn name(self) -> &'static str {
        self.0
    }

    // --- Core commands (views.h) ---
    /// `cmValid` — also the zero/default command.
    pub const VALID: Command = Command("tv.valid");
    /// `cmQuit`
    pub const QUIT: Command = Command("tv.quit");
    /// `cmError`
    pub const ERROR: Command = Command("tv.error");
    /// `cmMenu`
    pub const MENU: Command = Command("tv.menu");
    /// `cmClose`
    pub const CLOSE: Command = Command("tv.close");
    /// `cmZoom`
    pub const ZOOM: Command = Command("tv.zoom");
    /// `cmResize`
    pub const RESIZE: Command = Command("tv.resize");
    /// `cmNext`
    pub const NEXT: Command = Command("tv.next");
    /// `cmPrev`
    pub const PREV: Command = Command("tv.prev");
    /// `cmHelp`
    pub const HELP: Command = Command("tv.help");

    // --- Standard dialog result commands (dialogs.h) ---
    /// `cmOK`
    pub const OK: Command = Command("tv.ok");
    /// `cmCancel`
    pub const CANCEL: Command = Command("tv.cancel");
    /// `cmYes`
    pub const YES: Command = Command("tv.yes");
    /// `cmNo`
    pub const NO: Command = Command("tv.no");
    /// `cmDefault`
    pub const DEFAULT: Command = Command("tv.default");

    // --- Standard editing commands / clipboard (editors.h) ---
    /// `cmCut`
    pub const CUT: Command = Command("tv.cut");
    /// `cmCopy`
    pub const COPY: Command = Command("tv.copy");
    /// `cmPaste`
    pub const PASTE: Command = Command("tv.paste");
    /// `cmUndo`
    pub const UNDO: Command = Command("tv.undo");
    /// `cmClear`
    pub const CLEAR: Command = Command("tv.clear");

    // --- Window management (app.h) ---
    /// `cmTile`
    pub const TILE: Command = Command("tv.tile");
    /// `cmCascade`
    pub const CASCADE: Command = Command("tv.cascade");

    // --- Application menu commands (app.h) ---
    /// `cmNew`
    pub const NEW: Command = Command("tv.new");
    /// `cmOpen`
    pub const OPEN: Command = Command("tv.open");
    /// `cmSave`
    pub const SAVE: Command = Command("tv.save");
    /// `cmSaveAs`
    pub const SAVE_AS: Command = Command("tv.save_as");
    /// `cmSaveAll`
    pub const SAVE_ALL: Command = Command("tv.save_all");
    /// `cmChDir`
    pub const CH_DIR: Command = Command("tv.ch_dir");
    /// `cmDosShell`
    pub const DOS_SHELL: Command = Command("tv.dos_shell");
    /// `cmCloseAll`
    pub const CLOSE_ALL: Command = Command("tv.close_all");

    // --- Broadcast / message commands (views.h, dialogs.h) ---
    /// `cmReceivedFocus`
    pub const RECEIVED_FOCUS: Command = Command("tv.received_focus");
    /// `cmReleasedFocus`
    pub const RELEASED_FOCUS: Command = Command("tv.released_focus");
    /// `cmCommandSetChanged`
    pub const COMMAND_SET_CHANGED: Command = Command("tv.command_set_changed");
    /// `cmScrollBarChanged`
    pub const SCROLL_BAR_CHANGED: Command = Command("tv.scroll_bar_changed");
    /// `cmScrollBarClicked`
    pub const SCROLL_BAR_CLICKED: Command = Command("tv.scroll_bar_clicked");
    /// `cmSelectWindowNum`
    pub const SELECT_WINDOW_NUM: Command = Command("tv.select_window_num");
    /// `cmListItemSelected`
    pub const LIST_ITEM_SELECTED: Command = Command("tv.list_item_selected");
    /// `cmScreenChanged`
    pub const SCREEN_CHANGED: Command = Command("tv.screen_changed");
    /// `cmRecordHistory` (dialogs.h)
    pub const RECORD_HISTORY: Command = Command("tv.record_history");

    // --- Editor search/replace commands (editors.h) ---
    /// `cmFind`
    pub const FIND: Command = Command("tv.find");
    /// `cmReplace`
    pub const REPLACE: Command = Command("tv.replace");
    /// `cmSearchAgain`
    pub const SEARCH_AGAIN: Command = Command("tv.search_again");

    // --- Editor movement / edit commands (editors.h `cm*` 500..526) ---
    /// `cmCharLeft`
    pub const CHAR_LEFT: Command = Command("tv.char_left");
    /// `cmCharRight`
    pub const CHAR_RIGHT: Command = Command("tv.char_right");
    /// `cmWordLeft`
    pub const WORD_LEFT: Command = Command("tv.word_left");
    /// `cmWordRight`
    pub const WORD_RIGHT: Command = Command("tv.word_right");
    /// `cmLineStart`
    pub const LINE_START: Command = Command("tv.line_start");
    /// `cmLineEnd`
    pub const LINE_END: Command = Command("tv.line_end");
    /// `cmLineUp`
    pub const LINE_UP: Command = Command("tv.line_up");
    /// `cmLineDown`
    pub const LINE_DOWN: Command = Command("tv.line_down");
    /// `cmPageUp`
    pub const PAGE_UP: Command = Command("tv.page_up");
    /// `cmPageDown`
    pub const PAGE_DOWN: Command = Command("tv.page_down");
    /// `cmTextStart`
    pub const TEXT_START: Command = Command("tv.text_start");
    /// `cmTextEnd`
    pub const TEXT_END: Command = Command("tv.text_end");
    /// `cmNewLine`
    pub const NEW_LINE: Command = Command("tv.new_line");
    /// `cmBackSpace`
    pub const BACK_SPACE: Command = Command("tv.back_space");
    /// `cmDelChar`
    pub const DEL_CHAR: Command = Command("tv.del_char");
    /// `cmDelWord`
    pub const DEL_WORD: Command = Command("tv.del_word");
    /// `cmDelStart`
    pub const DEL_START: Command = Command("tv.del_start");
    /// `cmDelEnd`
    pub const DEL_END: Command = Command("tv.del_end");
    /// `cmDelLine`
    pub const DEL_LINE: Command = Command("tv.del_line");
    /// `cmInsMode`
    pub const INS_MODE: Command = Command("tv.ins_mode");
    /// `cmStartSelect`
    pub const START_SELECT: Command = Command("tv.start_select");
    /// `cmHideSelect`
    pub const HIDE_SELECT: Command = Command("tv.hide_select");
    /// `cmIndentMode`
    pub const INDENT_MODE: Command = Command("tv.indent_mode");
    /// `cmUpdateTitle`
    pub const UPDATE_TITLE: Command = Command("tv.update_title");
    /// `cmSelectAll`
    pub const SELECT_ALL: Command = Command("tv.select_all");
    /// `cmDelWordLeft`
    pub const DEL_WORD_LEFT: Command = Command("tv.del_word_left");
    /// `cmEncoding`
    pub const ENCODING: Command = Command("tv.encoding");

    // --- File-dialog commands (stddlg.h) ---
    /// `cmFileOpen` (stddlg.h `1001`)
    pub const FILE_OPEN: Command = Command("tv.file_open");
    /// `cmFileReplace` (stddlg.h `1002`)
    pub const FILE_REPLACE: Command = Command("tv.file_replace");
    /// `cmFileClear` (stddlg.h `1003`)
    pub const FILE_CLEAR: Command = Command("tv.file_clear");
    /// `cmFileInit` (stddlg.h `1004`)
    pub const FILE_INIT: Command = Command("tv.file_init");
    /// `cmChangeDir` (stddlg.h `1005`)
    pub const CHANGE_DIR: Command = Command("tv.change_dir");
    /// `cmRevert` (stddlg.h `1006`)
    pub const REVERT: Command = Command("tv.revert");
    /// `cmFileFocused` (stddlg.h `102`) — broadcast by `TFileList::focusItem` on
    /// every focus change; the focused file record is the payload (carried via
    /// the broadcast's resolvable `source`, resolved by the pump's
    /// `ResolveFocusedFile`).
    pub const FILE_FOCUSED: Command = Command("tv.file_focused");
    /// `cmFileDoubleClicked` (stddlg.h, next after `cmFileFocused`) — broadcast by
    /// `TFileList::selectItem`. Faithfully payload-less in rstv (the only consumer,
    /// `TFileDialog::handleEvent`, just turns it into `cmOK`).
    pub const FILE_DOUBLE_CLICKED: Command = Command("tv.file_double_clicked");

    /// `cmOutlineItemSelected = 301` (outline.h) — broadcast by
    /// `TOutlineViewer::selected` overrides (the base does nothing). Faithfully
    /// payload-less in rstv.
    pub const OUTLINE_ITEM_SELECTED: Command = Command("tv.outline_item_selected");

    // --- Theme editor commands (C8) ---
    /// Open the foreground color picker for the selected theme role
    /// (`ThemeEditorBody` Fg button / `f` hotkey). rstv-native.
    pub const THEME_EDIT_FG: Command = Command("tv.theme_edit_fg");
    /// Open the background color picker for the selected theme role
    /// (`ThemeEditorBody` Bg button / `b` hotkey). rstv-native.
    pub const THEME_EDIT_BG: Command = Command("tv.theme_edit_bg");
}

/// A set of commands the framework enables or disables in bulk.
///
/// The command space is **open/unbounded** (commands are namespaced strings,
/// not `0..=255`), so there is no trackable-range guard and no `all()`
/// constructor — "all commands" is not enumerable. The set itself is
/// polarity-neutral; the framework's **enabled-by-default policy** lives in
/// [`Program`](crate::Program), which keeps its current set as the complement —
/// a **disabled set** (a denylist). The `enable_cmd`/`disable_cmd` method names
/// port the C++ API and mean insert/remove regardless of which polarity a
/// particular owner stores; the polarity-neutral [`insert`](Self::insert) /
/// [`remove`](Self::remove) aliases are preferred at sites where the set's
/// meaning is not "enabled commands" (e.g. the disabled set).
///
/// # Turbo Vision heritage
///
/// Faithful to `TCommandSet` (`views.h`, `tcmdset.cpp`); the `uchar cmds[32]`
/// bit array (256 bits) becomes a [`HashSet<Command>`] (deviation D1).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct CommandSet {
    cmds: HashSet<Command>,
}

impl CommandSet {
    /// An empty command set. Ports `TCommandSet::TCommandSet()` (all bits zero).
    pub fn new() -> Self {
        CommandSet::default()
    }

    /// Whether `cmd` is enabled. Ports `TCommandSet::has`.
    pub fn has(&self, cmd: Command) -> bool {
        self.cmds.contains(&cmd)
    }

    /// Alias for [`has`](Self::has), matching Rust collection convention.
    pub fn contains(&self, cmd: Command) -> bool {
        self.has(cmd)
    }

    /// Enable a single command. Ports `TCommandSet::enableCmd(int)`.
    pub fn enable_cmd(&mut self, cmd: Command) {
        self.cmds.insert(cmd);
    }

    /// Disable a single command. Ports `TCommandSet::disableCmd(int)`.
    pub fn disable_cmd(&mut self, cmd: Command) {
        self.cmds.remove(&cmd);
    }

    /// Rust-collection-convention alias for [`enable_cmd`](Self::enable_cmd) —
    /// set membership, polarity-neutral; prefer it when the set's MEANING is
    /// not "enabled commands" (e.g. a disabled set).
    pub fn insert(&mut self, cmd: Command) {
        self.enable_cmd(cmd);
    }

    /// Rust-collection-convention alias for [`disable_cmd`](Self::disable_cmd) —
    /// set membership, polarity-neutral; prefer it when the set's MEANING is
    /// not "enabled commands" (e.g. a disabled set).
    pub fn remove(&mut self, cmd: Command) {
        self.disable_cmd(cmd);
    }

    /// Enable every command in `other` (set union). Ports
    /// `TCommandSet::enableCmd(const TCommandSet&)` / `operator |=`.
    pub fn enable_set(&mut self, other: &CommandSet) {
        self.cmds.extend(other.cmds.iter().copied());
    }

    /// Disable every command in `other` (set difference). Ports
    /// `TCommandSet::disableCmd(const TCommandSet&)`.
    pub fn disable_set(&mut self, other: &CommandSet) {
        for cmd in &other.cmds {
            self.cmds.remove(cmd);
        }
    }

    /// Whether no commands are enabled. Ports `TCommandSet::isEmpty`.
    pub fn is_empty(&self) -> bool {
        self.cmds.is_empty()
    }
}

// --- Operator ports (views.h inline operators + tcmdset.cpp friends) ---

/// Ports `TCommandSet::operator += (int)` → `enableCmd`.
impl AddAssign<Command> for CommandSet {
    fn add_assign(&mut self, cmd: Command) {
        self.enable_cmd(cmd);
    }
}

/// Ports `TCommandSet::operator -= (int)` → `disableCmd`.
impl SubAssign<Command> for CommandSet {
    fn sub_assign(&mut self, cmd: Command) {
        self.disable_cmd(cmd);
    }
}

/// Ports `TCommandSet::operator += (const TCommandSet&)` → `enableCmd` (union).
impl AddAssign<&CommandSet> for CommandSet {
    fn add_assign(&mut self, other: &CommandSet) {
        self.enable_set(other);
    }
}

/// Ports `TCommandSet::operator -= (const TCommandSet&)` → `disableCmd`.
impl SubAssign<&CommandSet> for CommandSet {
    fn sub_assign(&mut self, other: &CommandSet) {
        self.disable_set(other);
    }
}

/// Ports `TCommandSet::operator |= ` (set union).
impl BitOrAssign<&CommandSet> for CommandSet {
    fn bitor_assign(&mut self, other: &CommandSet) {
        self.enable_set(other);
    }
}

/// Ports `TCommandSet::operator &= ` (set intersection).
impl BitAndAssign<&CommandSet> for CommandSet {
    fn bitand_assign(&mut self, other: &CommandSet) {
        self.cmds.retain(|cmd| other.cmds.contains(cmd));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn custom_round_trips_and_namespaces() {
        let c = Command::custom("myapp.refresh");
        assert_eq!(c.name(), "myapp.refresh");

        // Two commands with equal-content names compare equal, regardless of
        // where the literals live.
        let a = Command::custom("myapp.refresh");
        let b = Command::custom(&"myapp.refresh".to_string().leak()[..]);
        assert_eq!(a, b);

        // Different namespaces (or names) differ.
        assert_ne!(Command::custom("a.refresh"), Command::custom("b.refresh"));
        assert_ne!(
            Command::custom("myapp.refresh"),
            Command::custom("myapp.reload")
        );
    }

    #[test]
    fn framework_constants_are_namespaced() {
        assert_eq!(Command::VALID.name(), "tv.valid");
        assert_eq!(Command::OK.name(), "tv.ok");
        assert_eq!(Command::SAVE_AS.name(), "tv.save_as");
        assert_eq!(
            Command::COMMAND_SET_CHANGED.name(),
            "tv.command_set_changed"
        );
    }

    #[test]
    fn default_command_is_valid() {
        assert_eq!(Command::default(), Command::VALID);
    }

    #[test]
    fn new_set_is_empty() {
        let cs = CommandSet::new();
        assert!(cs.is_empty());
        assert!(!cs.has(Command::OK));
    }

    #[test]
    fn enable_disable_has() {
        let mut cs = CommandSet::new();
        cs.enable_cmd(Command::OK);
        assert!(cs.has(Command::OK));
        assert!(cs.contains(Command::OK));
        cs.disable_cmd(Command::OK);
        assert!(!cs.has(Command::OK));
    }

    #[test]
    fn insert_remove_alias_enable_disable_cmd() {
        // The polarity-neutral aliases are behaviorally identical to the
        // faithful port names (insert == enable_cmd, remove == disable_cmd).
        let mut via_alias = CommandSet::new();
        let mut via_port = CommandSet::new();

        via_alias.insert(Command::OK);
        via_port.enable_cmd(Command::OK);
        via_alias.insert(Command::ZOOM);
        via_port.enable_cmd(Command::ZOOM);
        assert_eq!(via_alias, via_port);
        assert!(via_alias.has(Command::OK) && via_alias.has(Command::ZOOM));

        via_alias.remove(Command::OK);
        via_port.disable_cmd(Command::OK);
        assert_eq!(via_alias, via_port);
        assert!(!via_alias.has(Command::OK));
        assert!(via_alias.has(Command::ZOOM));

        // Idempotent like the underlying HashSet ops.
        via_alias.remove(Command::OK);
        via_port.disable_cmd(Command::OK);
        assert_eq!(via_alias, via_port);
    }

    #[test]
    fn custom_commands_are_tracked() {
        // The command space is open: a custom command can be enabled and is
        // reported by `has`, with no 0..=255 boundary behaviour.
        let mut cs = CommandSet::new();
        let refresh = Command::custom("x.y");
        cs.enable_cmd(refresh);
        assert!(cs.has(refresh));
        cs.disable_cmd(refresh);
        assert!(!cs.has(refresh));
        assert!(cs.is_empty());
    }

    #[test]
    fn add_sub_assign_command() {
        let mut cs = CommandSet::new();
        cs += Command::CUT;
        cs += Command::COPY;
        assert!(cs.has(Command::CUT));
        assert!(cs.has(Command::COPY));
        cs -= Command::CUT;
        assert!(!cs.has(Command::CUT));
        assert!(cs.has(Command::COPY));
    }

    #[test]
    fn union_via_enable_set_and_add_assign() {
        let mut a = CommandSet::new();
        a.enable_cmd(Command::OK);
        let mut b = CommandSet::new();
        b.enable_cmd(Command::CANCEL);
        b.enable_cmd(Command::YES);

        let mut viamethod = a.clone();
        viamethod.enable_set(&b);
        assert!(viamethod.has(Command::OK));
        assert!(viamethod.has(Command::CANCEL));
        assert!(viamethod.has(Command::YES));

        let mut viaop = a.clone();
        viaop += &b;
        assert_eq!(viaop, viamethod);

        let mut viabitor = a;
        viabitor |= &b;
        assert_eq!(viabitor, viamethod);
    }

    #[test]
    fn disable_set_and_sub_assign() {
        let mut a = CommandSet::new();
        a.enable_cmd(Command::OK);
        a.enable_cmd(Command::ZOOM);
        a.enable_cmd(Command::CLOSE);
        let mut remove = CommandSet::new();
        remove.enable_cmd(Command::ZOOM);
        remove.enable_cmd(Command::CLOSE);

        let mut viamethod = a.clone();
        viamethod.disable_set(&remove);
        assert!(!viamethod.has(Command::ZOOM));
        assert!(!viamethod.has(Command::CLOSE));
        assert!(viamethod.has(Command::OK));

        a -= &remove;
        assert_eq!(a, viamethod);
    }

    #[test]
    fn intersection_via_bitand_assign() {
        let mut a = CommandSet::new();
        a.enable_cmd(Command::OK);
        a.enable_cmd(Command::CANCEL);
        a.enable_cmd(Command::YES);

        let mut b = CommandSet::new();
        b.enable_cmd(Command::CANCEL);
        b.enable_cmd(Command::YES);
        b.enable_cmd(Command::NO);

        a &= &b;
        assert!(!a.has(Command::OK));
        assert!(a.has(Command::CANCEL));
        assert!(a.has(Command::YES));
        assert!(!a.has(Command::NO)); // only in b, not in original a
    }

    #[test]
    fn equality_and_is_empty() {
        let mut a = CommandSet::new();
        let mut b = CommandSet::new();
        assert_eq!(a, b);
        assert!(a.is_empty());

        a.enable_cmd(Command::OK);
        assert_ne!(a, b);
        assert!(!a.is_empty());

        b.enable_cmd(Command::OK);
        assert_eq!(a, b);
    }
}
