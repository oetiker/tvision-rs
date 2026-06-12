//! The shared, non-interactive layer behind the menu bar and menu boxes:
//! [`MenuViewState`] (the data every menu view holds) plus the free functions for
//! command-graying and passive accelerator dispatch.
//!
//! Two responsibilities live here:
//!
//! 1. The **command-graying broker** — `updateMenu`, driven by the
//!    `cmCommandSetChanged` broadcast: it walks the menu tree and marks each item
//!    enabled/disabled to match the live command set.
//! 2. **Passive accelerator dispatch** — the key branch of `handle_event` that
//!    posts the command of a menu item whose `keyCode` matches a pressed key, even
//!    when no menu is open (`hotKey`/`findHotKey`).
//!
//! The interactive modal layer (opening, navigating, and selecting) lives in
//! [`MenuBar`](crate::menu::MenuBar), [`MenuBox`](crate::menu::MenuBox), and the
//! [`MenuSession`](crate::menu) capture handler. The parent-of relationship
//! between an open box and the level above it is modeled by that session's level
//! stack rather than a field on the view, so `MenuViewState` carries no
//! `parentMenu` pointer.
//!
//! # Turbo Vision heritage
//! Ports the passive half of `TMenuView` (`tmnuview.cpp`/`menus.h`). The C++
//! `current`/`parentMenu` up-pointers become an item index plus the session's
//! level stack (deviation D3); the disabled-command set is held as a denylist
//! (deviation D1) and `TStreamable` persistence is dropped (deviation D12).

use crate::color::Style;
use crate::command::{Command, CommandSet};
use crate::event::{Event, KeyEvent};
use crate::menu::{Menu, MenuItem};
use crate::theme::Role;
use crate::view::{Context, DrawCtx, Rect, View, ViewState};

/// Runtime (view) state shared by the menu views — the `TMenuView` data members
/// that [`MenuBar`](crate::menu::MenuBar) and [`MenuBox`](crate::menu::MenuBox)
/// build on.
///
/// The `current` field (the highlighted item) is an **index** into the menu's
/// items; the draw layer reads it to pick the selected colour. The C++
/// `parentMenu` up-pointer has no field here — the parent-of relationship is held
/// by the menu session's level stack instead.
pub struct MenuViewState {
    /// The embedded [`ViewState`] (`TView` data members).
    pub state: ViewState,
    /// The menu tree this view presents (C++ `TMenuView::menu`).
    pub menu: Menu,
    /// `TMenuView::current` — the highlighted item, an **index** into
    /// [`menu`](Self::menu)`.items` (C++ `TMenuItem* current`; `None` == C++
    /// `current == 0`). Consistent with [`Menu::default`] (also an index). Draw
    /// compares `Some(i) == current` to pick the selected colour; defaults to
    /// `None` (nothing highlighted).
    ///
    /// `Option<usize>` fits every menu mutation: `current = menu->deflt` → index;
    /// `nextItem`/`prevItem` wrap by index; `current = p` → index;
    /// `menu->deflt = current; current = 0` → set default + `None`;
    /// `p == current` comparisons → index equality.
    pub current: Option<usize>,
}

impl MenuViewState {
    /// Build a menu-view state over `state` and `menu`, with nothing highlighted
    /// (`current = None`, the C++ `current == 0`). `current` is `pub`, so a caller
    /// (or test) can set it directly.
    pub fn new(state: ViewState, menu: Menu) -> Self {
        MenuViewState {
            state,
            menu,
            current: None,
        }
    }
}

/// The polymorphism seam between [`MenuBar`](crate::menu::MenuBar) and
/// [`MenuBox`](crate::menu::MenuBox).
///
/// `get_item_rect` and `draw` are the operations that differ between a bar and a
/// box, so (mirroring [`ListViewer`](crate::widgets::list_viewer::ListViewer)) the
/// abstract base is a trait carrying the data accessors plus those overridable
/// operations, while the passive shared logic
/// ([`hot_key`]/[`update_menu_commands`]/[`handle_event`]) stays as free functions
/// over `&Menu`/[`MenuViewState`]. The item geometry (`get_item_rect`) and the
/// draw layout are the same contract and must agree cell-for-cell, so they live
/// together; menu navigation calls `get_item_rect`/`draw`/`new_sub_view` through
/// `MenuView` references.
///
/// # Turbo Vision heritage
/// Ports the abstract part of `TMenuView` (`tmnuview.cpp`/`menus.h`). C++
/// inheritance (`TMenuBar`/`TMenuBox : TMenuView`) becomes a trait the two
/// concrete views implement (deviation D2).
pub trait MenuView: View {
    /// Borrow the embedded [`MenuViewState`].
    fn mv(&self) -> &MenuViewState;
    /// Mutably borrow the embedded [`MenuViewState`].
    fn mv_mut(&mut self) -> &mut MenuViewState;

    /// `TMenuView::getItemRect` — the screen rect of item `index` within this view.
    /// Base returns an empty rect (C++ `TRect(0,0,0,0)`);
    /// [`MenuBar`](crate::menu::MenuBar)/[`MenuBox`](crate::menu::MenuBox) override.
    fn get_item_rect(&self, _index: usize) -> Rect {
        Rect::new(0, 0, 0, 0)
    }
}

/// The four `(lo, hi)` style pairs a menu item is drawn in — the C++ `getColor`
/// matrix (`cNormal`/`cSelect`/`cNormDisabled`/`cSelDisabled`), resolved once per
/// `draw`. Shared by [`MenuBar`](crate::menu::MenuBar) and
/// [`MenuBox`](crate::menu::MenuBox) so the disabled/selected matrix lives in one
/// place.
#[derive(Clone, Copy)]
pub struct MenuColors {
    /// `cNormal = getColor(0x0301)` → `(MenuNormal, MenuNormalShortcut)`.
    pub normal: (Style, Style),
    /// `cSelect = getColor(0x0604)` → `(MenuSelected, MenuSelectedShortcut)`.
    pub select: (Style, Style),
    /// `cNormDisabled = getColor(0x0202)` → `MenuDisabled` for both lo and hi.
    pub norm_disabled: (Style, Style),
    /// `cSelDisabled = getColor(0x0505)` → `MenuSelectedDisabled` for both lo/hi.
    pub sel_disabled: (Style, Style),
}

impl MenuColors {
    /// Resolve the `cpMenuView` palette roles from the draw context's theme.
    pub fn resolve(ctx: &DrawCtx) -> Self {
        let d = ctx.style(Role::MenuDisabled);
        let sd = ctx.style(Role::MenuSelectedDisabled);
        MenuColors {
            normal: (
                ctx.style(Role::MenuNormal),
                ctx.style(Role::MenuNormalShortcut),
            ),
            select: (
                ctx.style(Role::MenuSelected),
                ctx.style(Role::MenuSelectedShortcut),
            ),
            // Disabled rows: a single style for both lo and hi (no shortcut
            // highlight when greyed).
            norm_disabled: (d, d),
            sel_disabled: (sd, sd),
        }
    }

    /// The `(lo, hi)` pair for an item given its `disabled`/`selected` state — the
    /// C++ `getColor` matrix, shared by command and submenu rows (bar and box).
    pub fn item(&self, disabled: bool, selected: bool) -> (Style, Style) {
        match (disabled, selected) {
            (true, true) => self.sel_disabled,
            (true, false) => self.norm_disabled,
            (false, true) => self.select,
            (false, false) => self.normal,
        }
    }
}

/// Find the menu item whose accelerator (`keyCode`) matches `key` and return its
/// [`Command`]. Ports `TMenuView::findHotKey` (via `hotKey`).
///
/// Faithful to the C++: walks the items in order, **skips separators**
/// (C++ `name == 0`), **recurses into submenus** (C++ `command == 0` → recurse
/// `subMenu->items`, regardless of the submenu's own `disabled` — the C++
/// `!p->disabled` guard is only on the command branch, and a submenu has no
/// command of its own to match), and matches a **command item** only when it is
/// not `disabled` and its `key_code` equals `Some(key)`. (`None` is the C++
/// `kbNoKey`, which never matches — already excluded by `Some(_) == key`.)
///
/// The first match wins (depth-first, in declaration order), as in the C++
/// recursive walk.
pub fn hot_key(menu: &Menu, key: KeyEvent) -> Option<Command> {
    for item in &menu.items {
        match item {
            // C++ name == 0: separator, skipped.
            MenuItem::Separator => {}
            // C++ command == 0: submenu — recurse only (do NOT match its own
            // key_code), regardless of the submenu's `disabled` flag.
            MenuItem::SubMenu { menu: sub, .. } => {
                if let Some(cmd) = hot_key(sub, key) {
                    return Some(cmd);
                }
            }
            // C++ command item: !disabled && keyCode != kbNoKey && keyCode == key.
            MenuItem::Command {
                command,
                key_code,
                disabled,
                ..
            } => {
                if !*disabled && *key_code == Some(key) {
                    return Some(*command);
                }
            }
        }
    }
    None
}

/// Regray the menu tree against the program's live **disabled-command set**.
/// Ports `TMenuView::updateMenu`.
///
/// `disabled_cmds` is the set of commands currently *disabled* (the complement
/// of C++'s `curCommandSet`). For each **command item** sets
/// `disabled = disabled_cmds.has(command)` (C++ `!commandEnabled(command)`);
/// **recurses into submenus** (a submenu's own `disabled` is never touched —
/// C++ updates only command items, recursing the submenu's items); **skips
/// separators**.
///
/// The C++ `Boolean updateMenu` returns whether anything changed (so
/// `handleEvent` can `drawView`). That return is **intentionally dropped**: the
/// whole tree is repainted each pump, so the conditional redraw is moot. The C++
/// guarded write (`if disabled == commandState` then flip) is equivalent to the
/// unconditional `disabled = !commandState` once the bool is dropped.
///
/// The C++ `if(menu != 0)` null-guard is moot in Rust: the [`Menu`] is owned, not
/// a nullable pointer.
pub fn update_menu_commands(menu: &mut Menu, disabled_cmds: &CommandSet) {
    for item in &mut menu.items {
        match item {
            MenuItem::Separator => {}
            MenuItem::SubMenu { menu: sub, .. } => {
                update_menu_commands(sub, disabled_cmds);
            }
            MenuItem::Command {
                command, disabled, ..
            } => {
                *disabled = disabled_cmds.has(*command);
            }
        }
    }
}

/// The **passive layer** of `TMenuView::handleEvent`.
///
/// Reads `mv.menu` + `mv.state.id()` and posts / requests through `ctx`; it does
/// **not** mutate the menu (regray is routed through the command-set broker). The
/// interactive *activation* branches (opening a menu, navigating it) live in the
/// menu session, not here, so this function leaves an activation event un-acted.
///
/// Handled branches:
/// - **`evBroadcast cmCommandSetChanged`** → request the regray broker by the
///   view's own id ([`Context::request_update_menu`]).
/// - **`evKeyDown`** → an accelerator match posts the item's command and clears
///   the event ([`hot_key`]); a bar alt-shortcut opens the menu session instead.
pub fn handle_event(mv: &MenuViewState, ev: &mut Event, ctx: &mut Context) {
    match ev {
        // C++ evBroadcast / cmCommandSetChanged: updateMenu(menu) (the conditional
        // drawView is moot under whole-tree redraw). The regray runs through the
        // broker — the menu view cannot read the command set inline, so request
        // UpdateMenu by our own id; the pump calls back through
        // View::update_menu_commands at apply time.
        //
        // NOTE (deviation): C++ TMenuView sets `eventMask |= evBroadcast` to opt
        // in to broadcasts. Our Group::handle_event fans broadcasts out to EVERY
        // child unconditionally (test
        // `broadcast_reaches_all_children_including_disabled`), so the menu
        // receives cmCommandSetChanged automatically — no mask/gate is ported.
        Event::Broadcast {
            command: Command::COMMAND_SET_CHANGED,
            ..
        } => {
            if let Some(id) = mv.state.id() {
                ctx.request_update_menu(id);
            }
        }
        // C++ evKeyDown (`TMenuView::handleEvent`). The C++ order is:
        // findAltShortcut → do_a_select (open the menu at the matched item) FIRST,
        // then fall back to the hotKey accelerator post.
        //
        // Only the **bar** (`size.y == 1`) activates: a box exists only inside an
        // active session,
        // which swallows its events on the capture stack, so a box never reaches
        // here live. The bar runs during group-routed preprocess dispatch
        // (`ofPreProcess`), so `ctx.owner_size()` is the root group size (C++
        // `owner->size`) and `mv.state` carries the bar's bounds — what
        // [`menu_session::activate`] needs.
        Event::KeyDown(k) => {
            // 1. Bar alt-shortcut → open the session at the matched item.
            if mv.state.size.y == 1
                && let Some(bar_id) = mv.state.id()
                && let Some(idx) = find_alt_shortcut_index(&mv.menu, k)
            {
                crate::menu::menu_session::activate(
                    bar_id,
                    mv.menu.clone(),
                    mv.state.get_bounds(),
                    ctx.owner_size(),
                    Some(idx),
                    ctx,
                );
                ev.clear();
                return;
            }
            // 2. Otherwise the hotKey accelerator post (the passive path).
            if let Some(cmd) = hot_key(&mv.menu, *k) {
                // C++ posts evCommand with the matched command and clears the
                // event. The C++ `commandEnabled(p->command)` re-check is NOT
                // ported: (a) hot_key's `!disabled` filter already excludes
                // disabled items, and that cached flag is kept current by the
                // regray broker; (b) even a stale-enabled post is dropped by the
                // pump's command boundary filter (program.rs: an Event::Command
                // whose cmd is in the disabled set is cleared before routing). The
                // only gap is a one-idle-cycle staleness window between a
                // command-set change and the next cmCommandSetChanged regray —
                // accepted.
                ctx.post(cmd);
                ev.clear();
            }
        }
        // evCommand cmMenu (kbF10 → cmMenu): the bar opens the session at the menu
        // default (`do_a_select`). Bar only (`size.y == 1`).
        Event::Command(Command::MENU) if mv.state.size.y == 1 => {
            if let Some(bar_id) = mv.state.id() {
                crate::menu::menu_session::activate(
                    bar_id,
                    mv.menu.clone(),
                    mv.state.get_bounds(),
                    ctx.owner_size(),
                    None, // cmMenu → menu->deflt
                    ctx,
                );
                ev.clear();
            }
        }
        // evMouseDown activation (`do_a_select`, tmnuview.cpp:505-516, reached from
        // handleEvent's evMouseDown arm :522-524): a click ON the bar opens the
        // session, which then (via the re-posted click) opens the clicked title's
        // box. Bar only (`size.y == 1`) and only when the click lands inside the
        // bar's bounds (a click elsewhere on the desktop must NOT activate). The
        // bar is at the root frame's `(0,0)`, so the position delivered here is
        // already root-frame.
        Event::MouseDown(m)
            if mv.state.size.y == 1
                && mv.state.id().is_some()
                && mv.state.get_bounds().contains(m.position) =>
        {
            let bar_id = mv.state.id().expect("guarded by id().is_some()");
            crate::menu::menu_session::activate_mouse(
                bar_id,
                mv.menu.clone(),
                mv.state.get_bounds(),
                ctx.owner_size(),
                *m,
                ctx,
            );
            ev.clear();
        }
        // Other un-handled events are inert (a box's events are owned by the
        // session on the capture stack; a bar click outside its bounds is not ours).
        _ => {}
    }
}

/// The shared "first **enabled, named** item whose hotkey letter matches `ke`"
/// walk — the common body of `findItem` (`tmnuview.cpp:420`, plain-letter,
/// `alt == false`) and the alt-char branch of `findAltShortcut`
/// (`tmnuview.cpp:436/441`, `alt == true`). Skips separators (C++ `name == 0`)
/// and disabled items (C++ `!p->disabled`), case-insensitive on the letter after
/// the first `~` (C++ `equalsIgnoreCase(ch, hotKeyStr(p->name))`).
///
/// `alt == true` matches an `Alt`-held key ([`is_alt_hotkey`](crate::event::is_alt_hotkey));
/// `alt == false` a plain unmodified press ([`is_plain_hotkey`](crate::event::is_plain_hotkey)).
/// Both predicates self-gate on the modifier, so no extra `alt` check is needed.
pub(crate) fn matching_item(menu: &Menu, ke: &KeyEvent, alt: bool) -> Option<usize> {
    for (i, item) in menu.items.iter().enumerate() {
        let (name, disabled) = match item {
            MenuItem::Separator => continue,
            MenuItem::Command { name, disabled, .. } | MenuItem::SubMenu { name, disabled, .. } => {
                (name.as_str(), *disabled)
            }
        };
        if disabled {
            continue;
        }
        if let Some(letter) = crate::event::hot_key(name) {
            let hit = if alt {
                crate::event::is_alt_hotkey(ke, letter)
            } else {
                crate::event::is_plain_hotkey(ke, letter)
            };
            if hit {
                return Some(i);
            }
        }
    }
    None
}

/// `TMenuView::findAltShortcut` (`tmnuview.cpp:436`), keyboard activation subset:
/// the index of the first **enabled, named** top-level item whose hotkey letter
/// matches the `Alt`-held `key`. Used by the bar's activation arm.
fn find_alt_shortcut_index(menu: &Menu, key: &KeyEvent) -> Option<usize> {
    matching_item(menu, key, true)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::command::CommandSet;
    use crate::event::{Key, KeyEvent};
    use crate::help::HelpCtx;
    use crate::menu::{Menu, MenuItem, alt};

    fn f3() -> KeyEvent {
        KeyEvent::from(Key::F(3))
    }
    fn f6() -> KeyEvent {
        KeyEvent::from(Key::F(6))
    }

    /// A File/Window menu: File has Open (F3), New (no key), a separator, and a
    /// disabled-via-key item; Window has a nested Next (F6).
    fn sample_menu() -> Menu {
        Menu::builder()
            .submenu("~F~ile", alt('f'), |m| {
                m.command_key("~O~pen", Command::OPEN, f3(), "F3")
                    .command("~N~ew", Command::NEW)
                    .separator()
            })
            .submenu("~W~indow", alt('w'), |m| {
                m.command_key("~N~ext", Command::NEXT, f6(), "F6")
            })
            .build()
    }

    // -- hot_key ------------------------------------------------------------

    #[test]
    fn hot_key_matches_top_level_via_submenu_descent() {
        // Open (F3) lives one level down in File; findHotKey recurses submenus.
        let menu = sample_menu();
        assert_eq!(hot_key(&menu, f3()), Some(Command::OPEN));
    }

    #[test]
    fn hot_key_recurses_into_a_second_submenu() {
        // Next (F6) lives in the Window submenu — proves recursion past the first
        // submenu into a later sibling submenu.
        let menu = sample_menu();
        assert_eq!(hot_key(&menu, f6()), Some(Command::NEXT));
    }

    #[test]
    fn hot_key_skips_disabled_item() {
        // Bite-check: with Open enabled the key matches; flipping `disabled`
        // makes the match disappear (proving the `!disabled` filter is live).
        let mut menu = sample_menu();
        assert_eq!(hot_key(&menu, f3()), Some(Command::OPEN));

        // Flip Open's `disabled` to true.
        if let MenuItem::SubMenu { menu: file, .. } = &mut menu.items[0] {
            *file.items[0]
                .disabled_mut()
                .expect("Open is a command item") = true;
        } else {
            panic!("items[0] should be the File submenu");
        }
        assert_eq!(
            hot_key(&menu, f3()),
            None,
            "a disabled item must not match (C++ !p->disabled)"
        );
    }

    #[test]
    fn hot_key_returns_none_for_separator_or_no_key() {
        // New has no accelerator (key_code None) and there is a separator; a key
        // that matches nothing returns None — and crucially the separator does
        // not panic / match.
        let menu = sample_menu();
        // A key nobody has.
        let unknown = KeyEvent::from(Key::F(12));
        assert_eq!(hot_key(&menu, unknown), None);
        // New's command must not be reachable by any key (it has key_code None,
        // which is C++ kbNoKey — never matches). Build a one-item menu to prove
        // a None key_code never matches a "None == Some" mishap.
        let only_new = Menu::builder().command("~N~ew", Command::NEW).build();
        // No KeyEvent equals a missing key_code.
        assert_eq!(hot_key(&only_new, f3()), None);
    }

    #[test]
    fn hot_key_does_not_match_a_submenus_own_key_code() {
        // The File submenu itself has key_code Alt-F. hot_key must NOT return on
        // a submenu's own accelerator (submenus carry no command) — it only
        // recurses. Searching for Alt-F finds nothing.
        let menu = sample_menu();
        assert_eq!(
            hot_key(&menu, alt('f')),
            None,
            "a submenu's own key_code is an open-shortcut, not a hot key"
        );
    }

    // -- update_menu_commands -----------------------------------------------

    #[test]
    fn update_menu_commands_regrays_recursively_against_set() {
        let mut menu = sample_menu();
        // The DISABLED set (denylist): New + Next disabled, Open left enabled.
        let mut disabled_cmds = CommandSet::new();
        disabled_cmds.insert(Command::NEW);
        disabled_cmds.insert(Command::NEXT);

        update_menu_commands(&mut menu, &disabled_cmds);

        // File > Open: NOT in the disabled set → disabled == false.
        // File > New: in the disabled set → disabled == true.
        let file = match &menu.items[0] {
            MenuItem::SubMenu { menu, .. } => menu,
            _ => panic!("items[0] is File"),
        };
        match &file.items[0] {
            MenuItem::Command {
                command, disabled, ..
            } => {
                assert_eq!(*command, Command::OPEN);
                assert!(!*disabled, "Open is not in the disabled set → enabled");
            }
            _ => panic!("File[0] is Open"),
        }
        match &file.items[1] {
            MenuItem::Command {
                command, disabled, ..
            } => {
                assert_eq!(*command, Command::NEW);
                assert!(*disabled, "New is in the disabled set → disabled");
            }
            _ => panic!("File[1] is New"),
        }
        // Window > Next (a SECOND-level submenu): in the disabled set →
        // disabled == true. Proves the recursion reaches a later sibling submenu.
        let window = match &menu.items[1] {
            MenuItem::SubMenu { menu, .. } => menu,
            _ => panic!("items[1] is Window"),
        };
        match &window.items[0] {
            MenuItem::Command {
                command, disabled, ..
            } => {
                assert_eq!(*command, Command::NEXT);
                assert!(*disabled, "Next is in the disabled set → disabled");
            }
            _ => panic!("Window[0] is Next"),
        }
    }

    #[test]
    fn update_menu_commands_predicate_is_plain_membership() {
        // Bite-check against the WRONG (allowlist-era) predicate
        // `disabled = !cs.has(command)`: with an EMPTY disabled set every item
        // must come out enabled. Under the negated predicate an item absent from
        // the set would gray, failing this assertion.
        let mut menu = Menu::builder()
            .item(MenuItem::SubMenu {
                name: "~F~ile".to_string(),
                key_code: None,
                help_ctx: HelpCtx::NO_CONTEXT,
                disabled: false,
                menu: Menu::builder().command("~S~ave", Command::SAVE).build(),
            })
            .build();
        // Pre-gray the inner item so the regray has to actively UN-gray it.
        if let MenuItem::SubMenu { menu: sub, .. } = &mut menu.items[0]
            && let MenuItem::Command { disabled, .. } = &mut sub.items[0]
        {
            *disabled = true;
        }
        let empty = CommandSet::new(); // nothing disabled

        update_menu_commands(&mut menu, &empty);

        match &menu.items[0] {
            MenuItem::SubMenu { menu: sub, .. } => match &sub.items[0] {
                MenuItem::Command { disabled, .. } => assert!(
                    !*disabled,
                    "command absent from the DISABLED set → enabled (plain membership)"
                ),
                _ => unreachable!(),
            },
            _ => unreachable!(),
        }
    }

    #[test]
    fn update_menu_commands_does_not_touch_submenu_disabled() {
        // A submenu's own `disabled` is never written (C++ updates only command
        // items). Start with a submenu marked disabled and assert it stays so
        // even though its inner command gets regrayed (to disabled, via the
        // denylist).
        let mut menu = Menu::builder()
            .item(MenuItem::SubMenu {
                name: "~F~ile".to_string(),
                key_code: None,
                help_ctx: HelpCtx::NO_CONTEXT,
                disabled: true, // deliberately set
                menu: Menu::builder().command("~O~pen", Command::OPEN).build(),
            })
            .build();
        let mut disabled_cmds = CommandSet::new();
        disabled_cmds.insert(Command::OPEN); // OPEN is disabled

        update_menu_commands(&mut menu, &disabled_cmds);

        match &menu.items[0] {
            MenuItem::SubMenu { disabled, menu, .. } => {
                assert!(*disabled, "the submenu's own disabled flag is left alone");
                match &menu.items[0] {
                    MenuItem::Command { disabled, .. } => {
                        assert!(*disabled, "the inner command WAS regrayed (disabled)")
                    }
                    _ => unreachable!(),
                }
            }
            _ => unreachable!(),
        }
    }
}
