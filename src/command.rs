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
//! **Guide:** [Commands & events](../../../apps/commands.html).
//!
//! # Turbo Vision heritage
//!
//! Ports the `cm*` command family and `TCommandSet` (`views.h`, `tcmdset.cpp`).
//! Commands were originally hand-assigned `int`s; here a command's identity is a
//! namespaced `&'static str` instead, so the command space is open and extensions
//! cannot collide (deviation D1). The integers existed only for serialization
//! (dropped) and to index a 256-bit set (now a [`HashSet`]), so commands no longer
//! carry a number.

use std::collections::HashSet;
use std::ops::{AddAssign, BitAndAssign, BitOrAssign, SubAssign};

/// The identity of an action — what a menu item, button, or key binding emits
/// and a view interprets. A command's identity is a **namespaced static
/// string** rather than a number, so downstream code can mint
/// application-specific commands collision-safely.
///
/// The field is private: a `Command` is an opaque token. The associated
/// constants below are the framework's standard vocabulary, and external
/// apps/views use [`Command::custom`] to define their own, namespacing under their
/// own dotted prefix:
///
/// ```
/// const REFRESH: tvision_rs::Command = tvision_rs::Command::custom("myapp.refresh");
/// ```
///
/// Equality and hashing compare the string *contents*, so two `Command`s with
/// equal-content names are equal regardless of where the literals live.
///
/// [`Default`] is [`Command::VALID`] (the zero command).
///
/// # Turbo Vision heritage
///
/// Ports the `cm*` command family (`views.h`), which were plain `int`s; here a
/// command's identity is a namespaced `&'static str` (deviation D1).
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

    // --- Core commands ---
    /// The no-op / always-valid command; also the zero/default command.
    ///
    /// Used as the response to [`View::valid`](crate::view::View::valid) meaning
    /// "this view is always willing to proceed". Also the [`Default`] value for
    /// `Command`.
    pub const VALID: Command = Command("tv.valid");
    /// Quit the application — emit from a menu item or key binding (e.g. Alt-X).
    ///
    /// Handled by [`Program`](crate::Program), which closes all windows and exits
    /// the event loop. Place in your status-line or File menu to give users a
    /// standard exit path.
    pub const QUIT: Command = Command("tv.quit");
    /// Report a runtime error — a beep / no-op fallback for unhandled commands.
    ///
    /// `Program` sends this to the focused view when a command cannot be routed;
    /// the default handler produces an audible bell. Views should not normally
    /// emit this directly.
    pub const ERROR: Command = Command("tv.error");
    /// Open the menu bar — emit from a key binding (conventionally F10).
    ///
    /// Handled by the `MenuBar`; the status-line default key set binds F10 to this
    /// command so users can enter the menu with the keyboard.
    pub const MENU: Command = Command("tv.menu");
    /// Close the active window — handled by the active [`Window`](crate::Window).
    ///
    /// Emitted by the window's close button (frame close icon) or from a menu item
    /// (e.g. Alt-F3 in the default window menu). The window calls
    /// [`View::valid`](crate::view::View::valid) before closing.
    pub const CLOSE: Command = Command("tv.close");
    /// Maximize / restore the active window — handled by the active [`Window`](crate::Window).
    ///
    /// Emitted by the window's zoom button (frame zoom icon) or a key binding.
    /// Toggles between the zoomed (full-desktop) rect and the saved un-zoom rect.
    pub const ZOOM: Command = Command("tv.zoom");
    /// Cycle the active window through frameless-fullscreen modes
    /// (`Off → Desktop → Screen → Off`). Handled by the [`Window`](crate::Window); no default key
    /// binding (apps bind their own). See [`Window::set_fullscreen`](crate::window::Window::set_fullscreen).
    pub const FULLSCREEN: Command = Command("tv.fullscreen");
    /// Enter window resize/move mode — handled by the active [`Window`](crate::Window).
    ///
    /// Emitted by the frame drag handle or a key binding. Starts the interactive
    /// drag loop where arrow keys reposition or resize the window.
    pub const RESIZE: Command = Command("tv.resize");
    /// Focus the next window on the desktop — handled by [`Program`](crate::Program).
    ///
    /// Cycles the desktop's current window forward. Bind to a key (e.g. F6) in
    /// the window menu to give keyboard access to window cycling.
    pub const NEXT: Command = Command("tv.next");
    /// Send the active window to the back (focus the previous one).
    ///
    /// Handled by [`Program`](crate::Program); cycles the desktop's current window
    /// backward. Bind to Shift-F6 or the window menu as a companion to `NEXT`.
    pub const PREV: Command = Command("tv.prev");
    /// Open context help — handled by [`Program`](crate::Program).
    ///
    /// Dispatched when the user presses F1 or the frame help icon. `Program`
    /// routes it to the registered help viewer if any.
    pub const HELP: Command = Command("tv.help");

    // --- Standard dialog result commands ---
    /// Confirm / accept a dialog — the modal result returned when the user presses
    /// the OK / default button or Enter.
    ///
    /// `Program::exec_view` returns this command when a dialog's modal loop ends on
    /// confirmation. Check the return value to distinguish OK from CANCEL after a
    /// `Dialog::run` / `MessageBox` call.
    pub const OK: Command = Command("tv.ok");
    /// Cancel / dismiss a dialog without accepting — always enabled (cannot be
    /// disabled via [`CommandSet`]).
    ///
    /// The Escape key generates this in a dialog. The framework guarantees
    /// `CANCEL` is always in the enabled command set so users can always dismiss.
    pub const CANCEL: Command = Command("tv.cancel");
    /// Answer "yes" in a yes/no confirmation dialog.
    ///
    /// Emitted by the Yes button in a `MessageBox::confirm` style dialog; check the
    /// modal result against this to detect a "yes" answer.
    pub const YES: Command = Command("tv.yes");
    /// Answer "no" in a yes/no confirmation dialog.
    ///
    /// Emitted by the No button in a `MessageBox::confirm` style dialog. The dialog
    /// loop ends and this is the return value when the user picks No.
    pub const NO: Command = Command("tv.no");
    /// Activate the dialog's default button — broadcast on Enter inside a dialog
    /// to the default button (the one whose `ButtonFlags.default` is true).
    pub const DEFAULT: Command = Command("tv.default");

    // --- Standard editing / clipboard commands ---
    /// Cut the selection to the clipboard (Shift-Del / Ctrl-X).
    ///
    /// Handled by `Editor` / `Memo`; disable via [`CommandSet`] when nothing is
    /// selected. Place in a standard Edit menu.
    pub const CUT: Command = Command("tv.cut");
    /// Copy the selection to the clipboard (Ctrl-Ins / Ctrl-C).
    ///
    /// Handled by `Editor` / `Memo`; disable via [`CommandSet`] when nothing is
    /// selected. Place in a standard Edit menu.
    pub const COPY: Command = Command("tv.copy");
    /// Paste from the clipboard (Shift-Ins / Ctrl-V).
    ///
    /// Handled by `Editor` / `Memo`. Place in a standard Edit menu.
    pub const PASTE: Command = Command("tv.paste");
    /// Undo the last edit (Alt-Backspace / Ctrl-Z).
    ///
    /// Handled by `Editor` / `Memo`; disable via [`CommandSet`] when the undo
    /// buffer is empty. Place in a standard Edit menu.
    pub const UNDO: Command = Command("tv.undo");
    /// Clear (delete) the selection without copying to the clipboard (Ctrl-Del).
    ///
    /// Handled by `Editor` / `Memo`; disable via [`CommandSet`] when nothing is
    /// selected.
    pub const CLEAR: Command = Command("tv.clear");

    // --- Window management ---
    /// Tile the open windows into a grid — handled by [`Desktop`](crate::Desktop).
    ///
    /// Bind to a Window menu item (e.g. "Tile"); the desktop repositions and
    /// resizes all tileable windows (`Options::tileable = true`) to fill the
    /// desktop area without overlap.
    pub const TILE: Command = Command("tv.tile");
    /// Cascade the open windows in an offset stack — handled by [`Desktop`](crate::Desktop).
    ///
    /// Bind to a Window menu item (e.g. "Cascade"); each tileable window is
    /// placed at a staggered offset so title bars are visible.
    pub const CASCADE: Command = Command("tv.cascade");

    // --- Application menu commands ---
    /// Create a new document — emit from a File menu "New" item.
    ///
    /// The framework does not handle this; your application handles it by
    /// opening a new editor window.
    pub const NEW: Command = Command("tv.new");
    /// Open a document — emit from a File menu "Open" item.
    ///
    /// The framework does not handle this automatically; your application handles
    /// it by running a file-open dialog and opening the chosen file.
    pub const OPEN: Command = Command("tv.open");
    /// Save the current document — emit from a File menu "Save" item.
    ///
    /// Your application handles this; disable via [`CommandSet`] when no document
    /// is open or the editor is unmodified.
    pub const SAVE: Command = Command("tv.save");
    /// Save the current document under a new name — emit from "Save As".
    ///
    /// Your application handles this by running a file-save dialog.
    pub const SAVE_AS: Command = Command("tv.save_as");
    /// Save all open documents — emit from "Save All".
    ///
    /// Your application handles this; disable when no modified documents are open.
    pub const SAVE_ALL: Command = Command("tv.save_all");
    /// Change the working directory — opens the change-directory dialog.
    ///
    /// Handled by the standard `ChDirDialog` when your application routes it.
    pub const CH_DIR: Command = Command("tv.ch_dir");
    /// Drop to a shell prompt (inherited from the DOS era; keep for compatibility).
    ///
    /// In the original Turbo Vision this suspended the application to a DOS
    /// shell; in a modern port you may repurpose it or leave it disabled.
    pub const DOS_SHELL: Command = Command("tv.dos_shell");
    /// Close all open windows — emit from a Window menu "Close All" item.
    ///
    /// [`Desktop`](crate::Desktop) / `Program` close every window whose
    /// `valid(CLOSE)` returns true.
    pub const CLOSE_ALL: Command = Command("tv.close_all");

    // --- Broadcast / message commands ---
    /// A view received focus — broadcast by `Group` whenever it calls
    /// `set_current`; carries the newly-focused view's `ViewId` as `source`.
    ///
    /// Listen for this in a parent or sibling to react to focus changes (e.g. to
    /// update a status bar or enable/disable commands).
    pub const RECEIVED_FOCUS: Command = Command("tv.received_focus");
    /// A view is about to release focus — broadcast before `set_current` removes
    /// focus; carries the old view's `ViewId` as `source`.
    ///
    /// Can be vetoed via [`View::valid`](crate::view::View::valid): if the focused
    /// view's `valid(RELEASED_FOCUS)` returns false, the focus change is blocked.
    pub const RELEASED_FOCUS: Command = Command("tv.released_focus");
    /// The set of enabled commands changed — broadcast by `Program` after any
    /// [`CommandSet`] update.
    ///
    /// Menu bars, status lines, and buttons listen for this to re-query which
    /// commands are enabled and redraw accordingly.
    pub const COMMAND_SET_CHANGED: Command = Command("tv.command_set_changed");
    /// A scroll bar's value changed — broadcast by [`ScrollBar`](crate::ScrollBar)
    /// whenever the thumb moves; `source` is the scroll bar's `ViewId`.
    ///
    /// A scroller or list viewer receives this broadcast and scrolls its content
    /// to match. The cross-view broker in the pump resolves `source` to the
    /// scroll bar and reads its value.
    pub const SCROLL_BAR_CHANGED: Command = Command("tv.scroll_bar_changed");
    /// A scroll bar arrow or page region was clicked — broadcast by
    /// [`ScrollBar`](crate::ScrollBar) to trigger a single scroll step.
    pub const SCROLL_BAR_CLICKED: Command = Command("tv.scroll_bar_clicked");
    /// Broadcast by a [`TabBar`](crate::widgets::TabBar) when its selected tab
    /// changes; carries the bar's own [`ViewId`](crate::view::ViewId) as the
    /// broadcast `source` so a sibling [`PageStack`](crate::widgets::PageStack)
    /// can tell which bar fired (the D3/D4 pattern, mirroring SCROLL_BAR_CHANGED).
    pub const TAB_BAR_CHANGED: Command = Command("tv.tab_bar_changed");
    /// Select the desktop window whose number matches the Alt-*N* keypress.
    ///
    /// Emitted by `Program` when the user presses Alt-1 through Alt-9; the
    /// desktop walks its children and focuses the window whose
    /// [`View::number`](crate::view::View::number) matches.
    pub const SELECT_WINDOW_NUM: Command = Command("tv.select_window_num");
    /// A list item was activated (broadcast) — emitted by `ListViewer` on
    /// Enter or double-click; `source` is the list viewer's `ViewId`.
    ///
    /// Listen for this broadcast in a parent view to react to item selection
    /// (e.g. open a document, populate a detail pane).
    pub const LIST_ITEM_SELECTED: Command = Command("tv.list_item_selected");
    /// The terminal size changed — broadcast by `Program` after a SIGWINCH /
    /// crossterm resize event; all views resize in response.
    ///
    /// Most views do not need to handle this directly; the framework resizes the
    /// view tree automatically.
    pub const SCREEN_CHANGED: Command = Command("tv.screen_changed");
    /// Record the current input-line value into its history list — handled by
    /// `InputLine` when it loses focus.
    ///
    /// `Program` broadcasts this before routing focus away from an input line so
    /// the line's history list is updated even if the user did not press Enter.
    pub const RECORD_HISTORY: Command = Command("tv.record_history");

    // --- Editor search/replace commands ---
    /// Open the find dialog — handled by `Editor`; bind to Ctrl-Q/F or Ctrl-F.
    pub const FIND: Command = Command("tv.find");
    /// Open the find-and-replace dialog — handled by `Editor`; bind to Ctrl-Q/A
    /// or Ctrl-H.
    pub const REPLACE: Command = Command("tv.replace");
    /// Repeat the last search from the current cursor position — handled by
    /// `Editor`; bind to Ctrl-L.
    pub const SEARCH_AGAIN: Command = Command("tv.search_again");

    // --- Editor movement / edit commands (handled by `Editor` / `Memo`) ---
    /// Move the cursor one character left (Left arrow / Ctrl-S in WordStar mode).
    pub const CHAR_LEFT: Command = Command("tv.char_left");
    /// Move the cursor one character right (Right arrow / Ctrl-D in WordStar mode).
    pub const CHAR_RIGHT: Command = Command("tv.char_right");
    /// Move the cursor one word left (Ctrl-Left / Ctrl-A in WordStar mode).
    pub const WORD_LEFT: Command = Command("tv.word_left");
    /// Move the cursor one word right (Ctrl-Right / Ctrl-F in WordStar mode).
    pub const WORD_RIGHT: Command = Command("tv.word_right");
    /// Move the cursor to the start of the line (Home / Ctrl-Q-S).
    pub const LINE_START: Command = Command("tv.line_start");
    /// Move the cursor to the end of the line (End / Ctrl-Q-D).
    pub const LINE_END: Command = Command("tv.line_end");
    /// Move the cursor up one line (Up arrow / Ctrl-E in WordStar mode).
    pub const LINE_UP: Command = Command("tv.line_up");
    /// Move the cursor down one line (Down arrow / Ctrl-X in WordStar mode).
    pub const LINE_DOWN: Command = Command("tv.line_down");
    /// Scroll up one page (PgUp / Ctrl-R in WordStar mode).
    pub const PAGE_UP: Command = Command("tv.page_up");
    /// Scroll down one page (PgDn / Ctrl-C in WordStar mode).
    pub const PAGE_DOWN: Command = Command("tv.page_down");
    /// Move the cursor to the start of the document (Ctrl-Home / Ctrl-Q-R).
    pub const TEXT_START: Command = Command("tv.text_start");
    /// Move the cursor to the end of the document (Ctrl-End / Ctrl-Q-C).
    pub const TEXT_END: Command = Command("tv.text_end");
    /// Insert a line break at the cursor position (Enter).
    pub const NEW_LINE: Command = Command("tv.new_line");
    /// Delete the character before the cursor (Backspace / Ctrl-H).
    pub const BACK_SPACE: Command = Command("tv.back_space");
    /// Delete the character at the cursor position (Delete / Ctrl-G).
    pub const DEL_CHAR: Command = Command("tv.del_char");
    /// Delete the word at (to the right of) the cursor (Ctrl-T in WordStar mode).
    pub const DEL_WORD: Command = Command("tv.del_word");
    /// Delete from the cursor to the start of the line (Ctrl-Q-Backspace).
    pub const DEL_START: Command = Command("tv.del_start");
    /// Delete from the cursor to the end of the line (Ctrl-Q-Y).
    pub const DEL_END: Command = Command("tv.del_end");
    /// Delete the current line (Ctrl-Y).
    pub const DEL_LINE: Command = Command("tv.del_line");
    /// Toggle insert / overwrite mode (Ins key).
    pub const INS_MODE: Command = Command("tv.ins_mode");
    /// Begin a keyboard selection (Ctrl-K-B).
    pub const START_SELECT: Command = Command("tv.start_select");
    /// Collapse / hide the current selection without deleting it (Ctrl-K-H).
    pub const HIDE_SELECT: Command = Command("tv.hide_select");
    /// Toggle auto-indent mode (Ctrl-O-I).
    pub const INDENT_MODE: Command = Command("tv.indent_mode");
    /// Update the editor window's title bar — emitted by `Editor` after a save
    /// or filename change so the frame rerenders the new title.
    pub const UPDATE_TITLE: Command = Command("tv.update_title");
    /// Select the entire document contents (Ctrl-A / Ctrl-K-K after Ctrl-K-B).
    pub const SELECT_ALL: Command = Command("tv.select_all");
    /// Delete the word to the left of the cursor (Ctrl-Backspace).
    pub const DEL_WORD_LEFT: Command = Command("tv.del_word_left");
    /// Change the text encoding of the current editor document.
    pub const ENCODING: Command = Command("tv.encoding");

    // --- File-dialog commands ---
    /// The file dialog's Open button.
    pub const FILE_OPEN: Command = Command("tv.file_open");
    /// The file dialog's Replace button.
    pub const FILE_REPLACE: Command = Command("tv.file_replace");
    /// The file dialog's Clear button.
    pub const FILE_CLEAR: Command = Command("tv.file_clear");
    /// Re-read the file dialog's directory listing.
    pub const FILE_INIT: Command = Command("tv.file_init");
    /// Confirm a directory change in the change-directory dialog.
    pub const CHANGE_DIR: Command = Command("tv.change_dir");
    /// Revert the change-directory dialog to the current directory.
    pub const REVERT: Command = Command("tv.revert");
    /// A file in the file list gained focus (broadcast on every focus change; the
    /// focused file record is carried via the broadcast's resolvable `source`).
    pub const FILE_FOCUSED: Command = Command("tv.file_focused");
    /// A file in the file list was double-clicked (broadcast). Payload-less in
    /// tvision-rs: the file dialog just turns it into [`OK`](Command::OK).
    pub const FILE_DOUBLE_CLICKED: Command = Command("tv.file_double_clicked");

    /// An outline-viewer item was selected — broadcast by
    /// [`Outline`](crate::widgets::Outline) when the user presses Enter or
    /// double-clicks a node.
    ///
    /// `source` is the `Outline`'s `ViewId`. Listen for this broadcast in a parent
    /// or sibling to react to node activation (e.g. open the selected item). The
    /// selected node is the one at `OutlineViewerState::foc`; resolve the outline's
    /// `ViewId` to read it.
    ///
    /// # Turbo Vision heritage
    /// Ports `cmOutlineItemSelected` (`outline.h`).
    pub const OUTLINE_ITEM_SELECTED: Command = Command("tv.outline_item_selected");

    // --- Theme editor commands ---
    /// Open the foreground color picker for the selected theme role (tvision-rs-native).
    pub const THEME_EDIT_FG: Command = Command("tv.theme_edit_fg");
    /// Open the background color picker for the selected theme role (tvision-rs-native).
    pub const THEME_EDIT_BG: Command = Command("tv.theme_edit_bg");
}

/// A set of commands the framework enables or disables in bulk.
///
/// The command space is **open/unbounded** (commands are namespaced strings,
/// not `0..=255`), so there is no trackable-range guard and no `all()`
/// constructor — "all commands" is not enumerable. The set itself is
/// polarity-neutral; the framework's **enabled-by-default policy** lives in
/// [`Program`](crate::Program), which keeps its current set as the complement —
/// a **disabled set** (a denylist). The `enable_cmd`/`disable_cmd` methods mean
/// insert/remove regardless of which polarity a particular owner stores; the
/// polarity-neutral [`insert`](Self::insert) / [`remove`](Self::remove) aliases
/// are preferred at sites where the set's meaning is not "enabled commands" (e.g.
/// the disabled set).
///
/// # Turbo Vision heritage
///
/// Ports `TCommandSet` (`views.h`, `tcmdset.cpp`); the 256-bit array becomes a
/// [`HashSet<Command>`] (deviation D1).
#[derive(Clone, PartialEq, Eq, Debug, Default)]
pub struct CommandSet {
    cmds: HashSet<Command>,
}

impl CommandSet {
    /// Build an empty command set.
    ///
    /// The returned set contains no commands. Populate it incrementally with
    /// [`enable_cmd`](Self::enable_cmd) / [`insert`](Self::insert) or the `+=`
    /// operator, or use it as the start of a denylist by calling
    /// [`disable_cmd`](Self::disable_cmd) / [`insert`](Self::insert) for each
    /// command that should be blocked.
    pub fn new() -> Self {
        CommandSet::default()
    }

    /// Test whether `cmd` is a member of this set.
    ///
    /// Returns `true` when the command was previously added and not yet
    /// removed. On a **disabled set** (denylist) the result means the command
    /// is blocked; on an **enabled set** it means the command is active — the
    /// interpretation depends on the set's role, not this method.
    ///
    /// Use [`contains`](Self::contains) at new call sites as the idiomatic
    /// Rust name; use `has` when mirroring the C++ `TCommandSet::has` call.
    pub fn has(&self, cmd: Command) -> bool {
        self.cmds.contains(&cmd)
    }

    /// Idiomatic alias for [`has`](Self::has) — follows Rust collection
    /// convention (`HashSet::contains`). Prefer this name at new call sites.
    pub fn contains(&self, cmd: Command) -> bool {
        self.has(cmd)
    }

    /// Add `cmd` to this set (semantic name: enable the command).
    ///
    /// When `self` is an **enabled set**, this marks the command as enabled
    /// and it will pass the framework's enabled-command check. When `self` is
    /// a **disabled set** (denylist — the pattern [`Program`](crate::Program)
    /// uses internally), prefer the polarity-neutral
    /// [`insert`](Self::insert) alias to avoid misleading code.
    ///
    /// The `+=` operator (`AddAssign<Command>`) is equivalent.
    pub fn enable_cmd(&mut self, cmd: Command) {
        self.cmds.insert(cmd);
    }

    /// Remove `cmd` from this set (semantic name: disable the command).
    ///
    /// When `self` is an **enabled set**, this marks the command as disabled
    /// so it will be greyed out in menus and rejected by status-line items.
    /// When `self` is a **disabled set** (denylist), prefer the
    /// polarity-neutral [`remove`](Self::remove) alias instead.
    ///
    /// The `-=` operator (`SubAssign<Command>`) is equivalent.
    pub fn disable_cmd(&mut self, cmd: Command) {
        self.cmds.remove(&cmd);
    }

    /// Polarity-neutral alias for [`enable_cmd`](Self::enable_cmd): add `cmd`
    /// to the set.
    ///
    /// Prefer `insert` over `enable_cmd` when the set's meaning is not "which
    /// commands are currently enabled" — for example, when building a denylist
    /// of blocked commands, `disabled.insert(cmd)` is clearer than
    /// `disabled.enable_cmd(cmd)`.
    pub fn insert(&mut self, cmd: Command) {
        self.enable_cmd(cmd);
    }

    /// Polarity-neutral alias for [`disable_cmd`](Self::disable_cmd): remove
    /// `cmd` from the set.
    ///
    /// Prefer `remove` over `disable_cmd` at denylist sites — see
    /// [`insert`](Self::insert) for the naming rationale.
    pub fn remove(&mut self, cmd: Command) {
        self.disable_cmd(cmd);
    }

    /// Add every command in `other` to this set (set union).
    ///
    /// After this call, `self` contains every command it previously held plus
    /// every command in `other`. Use to merge two enabled sets, or to
    /// re-enable a batch of commands at a denylist site (remove them from the
    /// blocked list by calling `disabled.disable_set(&to_restore)`).
    ///
    /// The `+=` / `|=` operators (`AddAssign<&CommandSet>` /
    /// `BitOrAssign<&CommandSet>`) are equivalent.
    pub fn enable_set(&mut self, other: &CommandSet) {
        self.cmds.extend(other.cmds.iter().copied());
    }

    /// Remove every command in `other` from this set (set difference).
    ///
    /// After this call, `self` no longer contains any command that appears in
    /// `other`. Use to suppress a batch of commands in an enabled set, or to
    /// block a batch of commands by adding them to a denylist.
    ///
    /// The `-=` operator (`SubAssign<&CommandSet>`) is equivalent.
    pub fn disable_set(&mut self, other: &CommandSet) {
        for cmd in &other.cmds {
            self.cmds.remove(cmd);
        }
    }

    /// Whether no commands are enabled.
    pub fn is_empty(&self) -> bool {
        self.cmds.is_empty()
    }
}

// --- Operator overloads ---

/// `set += cmd` — add a single command (equivalent to [`enable_cmd`](CommandSet::enable_cmd) /
/// [`insert`](CommandSet::insert)). Use this operator when building a set incrementally:
/// `let mut s = CommandSet::new(); s += Command::CUT; s += Command::COPY;`
impl AddAssign<Command> for CommandSet {
    fn add_assign(&mut self, cmd: Command) {
        self.enable_cmd(cmd);
    }
}

/// `set -= cmd` — remove a single command (equivalent to [`disable_cmd`](CommandSet::disable_cmd) /
/// [`remove`](CommandSet::remove)). Use to revoke a single command from an enabled set or to
/// unblock a single command from a denylist.
impl SubAssign<Command> for CommandSet {
    fn sub_assign(&mut self, cmd: Command) {
        self.disable_cmd(cmd);
    }
}

/// `set += other` — add every command in `other` (set union, equivalent to
/// [`enable_set`](CommandSet::enable_set)). Idiomatic C++ Pascal set `+` operator.
impl AddAssign<&CommandSet> for CommandSet {
    fn add_assign(&mut self, other: &CommandSet) {
        self.enable_set(other);
    }
}

/// `set -= other` — remove every command in `other` (set difference, equivalent to
/// [`disable_set`](CommandSet::disable_set)). Idiomatic C++ Pascal set `-` operator.
impl SubAssign<&CommandSet> for CommandSet {
    fn sub_assign(&mut self, other: &CommandSet) {
        self.disable_set(other);
    }
}

/// `set |= other` — set union (alias for `+= other`); same as
/// [`enable_set`](CommandSet::enable_set). The `|=` spelling emphasizes
/// the Boolean-OR semantics over the Pascal-add semantics.
impl BitOrAssign<&CommandSet> for CommandSet {
    fn bitor_assign(&mut self, other: &CommandSet) {
        self.enable_set(other);
    }
}

/// `set &= other` — set intersection: retains only commands present in **both**
/// `self` and `other`. Equivalent to the Pascal `*` operator on sets. Use when
/// you need the overlap of two command sets — for example, to find which commands
/// are enabled in all active views simultaneously.
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
