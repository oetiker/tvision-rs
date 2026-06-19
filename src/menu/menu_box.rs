//! The drop-down / pop-up menu box.
//!
//! [`MenuBox`] is a framed, shadowed [`MenuView`] that lays its items out
//! vertically, one per row. The box-specific work is its layout: the static
//! [`menu_box_rect`] sizing helper, [`frame_line`](MenuBox::frame_line),
//! [`draw`](MenuBox::draw), and [`get_item_rect`](MenuView::get_item_rect). Its
//! [`handle_event`](MenuBox::handle_event) delegates to the shared passive
//! [`menu_view::handle_event`]; the interactive navigation runs in the menu
//! session that owns the open box.
//!
//! ## The inset frame (intentional, **not** a bug)
//!
//! [`frame_line`](MenuBox::frame_line) writes the corner/edge glyph at columns 1
//! and `size.x-2`, leaving **column 0 and column `size.x-1` blank**. The menu-box
//! frame is deliberately inset one column on each side (those outer columns are
//! spaces in the frame-glyph table), and the snapshot captures it. Do not
//! "correct" it.
//!
//! ## The per-line colour fill
//!
//! Each frame row fills the **interior** columns `[2, size.x-2)` with a per-line
//! colour (its lo style), while the border cells keep the normal box style. A
//! selected or disabled item row highlights its interior by passing that colour
//! into the middle frame row, then the name is drawn over it in the same colour.
//!
//! As with [`MenuBar`](crate::menu::MenuBar), [`MenuViewState`] embeds a
//! [`ViewState`] (not a `View`), so the differing [`View`] methods are
//! hand-written rather than generated.
//!
//! # Turbo Vision heritage
//! Ports `TMenuBox` (`tmenubox.cpp`/`menus.h`). The `TMenuView` base becomes the
//! [`MenuView`] trait (deviation D2); `TStreamable` persistence is dropped
//! (deviation D12).

use crate::event::Event;
use crate::menu::menu_view::{self, MenuColors, MenuView, MenuViewState};
use crate::menu::{Menu, MenuItem};
use crate::view::{Context, DrawCtx, Rect, View, ViewState};

/// Display width of a `~`-marked label, ignoring the `~` markers. Per-module copy
/// mirroring `button.rs` (see [`MenuBar`]).
///
/// [`MenuBar`]: crate::menu::MenuBar
fn cstrlen(s: &str) -> i32 {
    s.chars()
        .filter(|&c| c != '~')
        .map(|c| unicode_width::UnicodeWidthChar::width(c).unwrap_or(1) as i32)
        .sum()
}

/// Compute the box bounds that fit `menu` inside `bounds`.
///
/// **Width** starts at a floor of 10 and grows to the widest item. The per-item
/// column budget is:
/// - command with shortcut text: `cstrlen(name) + 6 + cstrlen(param) + 2`
/// - submenu entry: `cstrlen(name) + 6 + 3` (the `+3` reserves a column for `►`)
/// - command without shortcut: `cstrlen(name) + 6`
/// - separator: no width contribution (but still adds a row)
///
/// **Height** is `2 + item_count` — 2 for the top and bottom borders plus one
/// row per item, separators included. The computed `(width, height)` is then
/// clamped into `bounds`: the box prefers its preferred size but is pushed
/// against the far edge when the available space is smaller.
///
/// The menu session calls this to size each submenu box as it opens; you can
/// also call it directly to pre-compute a popup box's bounds before passing them
/// to [`popup_menu`](crate::menu::popup_menu).
///
/// # Turbo Vision heritage
/// Ports the static `getRect` sizing helper (`tmenubox.cpp`).
pub fn menu_box_rect(bounds: Rect, menu: &Menu) -> Rect {
    let mut w = 10;
    let mut h = 2;
    for item in &menu.items {
        match item {
            MenuItem::Separator => {} // no width, but still takes a row (h++ below).
            MenuItem::SubMenu { name, .. } => {
                // a submenu: + 3 for the "►" marker column.
                let l = cstrlen(name) + 6 + 3;
                w = l.max(w);
            }
            MenuItem::Command { name, param, .. } => {
                let mut l = cstrlen(name) + 6;
                if let Some(p) = param {
                    // shortcut text: + its width + 2 for the right-aligned shortcut.
                    l += cstrlen(p) + 2;
                }
                w = l.max(w);
            }
        }
        h += 1; // every item takes a row.
    }

    let mut r = bounds;
    if r.a.x + w < r.b.x {
        r.b.x = r.a.x + w;
    } else {
        r.a.x = r.b.x - w;
    }
    if r.a.y + h < r.b.y {
        r.b.y = r.a.y + h;
    } else {
        r.a.y = r.b.y - h;
    }
    r
}

/// The framed, shadowed vertical menu box. Holds the shared [`MenuViewState`];
/// the box-specific layout lives in the methods below.
///
/// # Turbo Vision heritage
/// Ports `TMenuBox` (`tmenubox.cpp`/`menus.h`).
pub struct MenuBox {
    mv: MenuViewState,
}

impl MenuBox {
    /// Construct a menu box presenting `menu`, sized by [`menu_box_rect`] to fit
    /// inside `bounds`.
    ///
    /// `bounds` is the hint rect within which the box must fit — typically the
    /// remaining desktop area below and to the right of the parent item. The
    /// actual bounds are computed by [`menu_box_rect`] (the box shrinks to its
    /// content). The box casts a drop shadow (`state.shadow = true`) and
    /// pre-processes events (`options.pre_process = true`) so it intercepts
    /// accelerator keys before the focused view.
    ///
    /// Under normal operation the menu session constructs boxes on your behalf
    /// via `Deferred::OpenMenuBox`; call `MenuBox::new` directly only in tests
    /// or when building a standalone draw fixture.
    pub fn new(bounds: Rect, menu: Menu) -> Self {
        let rect = menu_box_rect(bounds, &menu);
        let mut state = ViewState::new(rect);
        state.state.shadow = true; // drop shadow
        state.options.pre_process = true; // see accelerators before focused view
        MenuBox {
            mv: MenuViewState::new(state, menu),
        }
    }

    /// Draw one box-frame row of style `kind`, with the interior columns in
    /// `color` and the border cells in `c_normal`.
    ///
    /// The single-line box glyphs (all from [`Glyphs`](crate::theme::Glyphs)):
    /// ```text
    /// kind  cols 0,1        cols [2, size.x-2)   cols size.x-2, size.x-1
    /// Top   ' '  ┌          ─                    ┐  ' '
    /// Bot   ' '  └          ─                    ┘  ' '
    /// Mid   ' '  │          ' '                  │  ' '
    /// Sep   ' '  ├          ─                    ┤  ' '
    /// ```
    /// Columns 0 and `size.x-1` are blank — the intentional inset (see the module
    /// doc).
    fn frame_line(
        ctx: &mut DrawCtx,
        size_x: i32,
        y: i32,
        kind: FrameKind,
        c_normal: crate::color::Style,
        color: crate::color::Style,
    ) {
        let g = ctx.glyphs();
        let (g0, g1, fill, g3, g4) = match kind {
            // n=0:  ' ' ┌ ─ ┐ ' '
            FrameKind::Top => (' ', g.frame_tl, g.frame_h, g.frame_tr, ' '),
            // n=5:  ' ' └ ─ ┘ ' '
            FrameKind::Bottom => (' ', g.frame_bl, g.frame_h, g.frame_br, ' '),
            // n=10: ' ' │ ' ' │ ' '
            FrameKind::Middle => (' ', g.frame_v, ' ', g.frame_v, ' '),
            // n=15: ' ' ├ ─ ┤ ' '
            FrameKind::Separator => (' ', g.frame_tee_l, g.frame_h, g.frame_tee_r, ' '),
        };
        // b.moveBuf(0, &frameChars[n], cNormal, 2) — cols 0, 1 in cNormal.
        ctx.put_char(0, y, g0, c_normal);
        ctx.put_char(1, y, g1, c_normal);
        // b.moveChar(2, frameChars[n+2], color, size.x - 4) — interior in `color`.
        ctx.fill(Rect::new(2, y, size_x - 2, y + 1), fill, color);
        // b.moveBuf(size.x-2, &frameChars[n+3], cNormal, 2) — last two in cNormal.
        ctx.put_char(size_x - 2, y, g3, c_normal);
        ctx.put_char(size_x - 1, y, g4, c_normal);
    }
}

/// Which frame row to draw.
#[derive(Clone, Copy)]
enum FrameKind {
    /// The top border.
    Top,
    /// The bottom border.
    Bottom,
    /// An item row (vertical edges, blank interior).
    Middle,
    /// A separator row (a `├──┤` divider).
    Separator,
}

impl MenuView for MenuBox {
    fn mv(&self) -> &MenuViewState {
        &self.mv
    }

    fn mv_mut(&mut self) -> &mut MenuViewState {
        &mut self.mv
    }

    /// The rect of item `index`, counting rows from `y = 1` (just below the top
    /// border).
    ///
    /// Every item — separators included — occupies exactly one row, so the row is
    /// the closed form `y = 1 + index`, and the rect spans the interior columns
    /// `[2, size.x - 2)`. (Unlike the bar's layout, where separators consume no
    /// horizontal space and a running walk is genuinely needed.)
    fn get_item_rect(&self, index: usize) -> Rect {
        let y = 1 + index as i32;
        let size_x = self.mv.state.size.x;
        Rect::new(2, y, size_x - 2, y + 1)
    }
}

impl View for MenuBox {
    fn state(&self) -> &ViewState {
        &self.mv.state
    }

    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.mv.state
    }

    /// Draw the top border, one row per item, then the bottom border.
    ///
    /// The colours come from [`MenuColors`] (the same matrix as
    /// [`MenuBar`](crate::menu::MenuBar)). For each named item the per-line colour
    /// is set to the selected/disabled lo style (or stays the normal box style),
    /// the frame row is drawn with its interior filled in that colour, then the
    /// label is drawn over it. A submenu gets the `►` marker near the right edge; a
    /// command with shortcut text gets that text right-aligned.
    fn draw(&mut self, ctx: &mut DrawCtx) {
        let colors = MenuColors::resolve(ctx);
        let c_normal = colors.normal.0; // cNormal lo — the border style.
        let marker = ctx.glyphs().input_right_arrow; // ► (CP437 \x10)

        let size = self.mv.state.size;
        let mut y = 0;

        // Top border (n = 0), interior in cNormal.
        MenuBox::frame_line(ctx, size.x, y, FrameKind::Top, c_normal, c_normal);
        y += 1;

        for (i, item) in self.mv.menu.items.iter().enumerate() {
            match item {
                MenuItem::Separator => {
                    // C++ p->name == 0: a separator row (n = 15), interior in cNormal.
                    MenuBox::frame_line(ctx, size.x, y, FrameKind::Separator, c_normal, c_normal);
                }
                MenuItem::Command {
                    name,
                    param,
                    disabled,
                    ..
                } => {
                    let selected = self.mv.current == Some(i);
                    // The per-line color/hi (cNormal unless selected/disabled).
                    let (lo, hi) = colors.item(*disabled, selected);
                    // frameLine(10): interior filled in `lo`, then the name over it.
                    MenuBox::frame_line(ctx, size.x, y, FrameKind::Middle, c_normal, lo);
                    ctx.put_cstr(3, y, name, lo, hi);
                    if let Some(p) = param {
                        // b.moveCStr(size.x-3-cstrlen(param), param, color).
                        ctx.put_cstr(size.x - 3 - cstrlen(p), y, p, lo, hi);
                    }
                }
                MenuItem::SubMenu { name, disabled, .. } => {
                    let selected = self.mv.current == Some(i);
                    let (lo, hi) = colors.item(*disabled, selected);
                    MenuBox::frame_line(ctx, size.x, y, FrameKind::Middle, c_normal, lo);
                    ctx.put_cstr(3, y, name, lo, hi);
                    // command == 0 (submenu): b.putChar(size.x-4, ►).
                    ctx.put_char(size.x - 4, y, marker, lo);
                }
            }
            y += 1;
        }

        // Bottom border (n = 5), interior in cNormal.
        MenuBox::frame_line(ctx, size.x, y, FrameKind::Bottom, c_normal, c_normal);
    }

    /// Delegates to the passive [`menu_view::handle_event`]; the box adds no event
    /// logic of its own (interactive navigation lives in the menu session).
    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        menu_view::handle_event(&self.mv, ev, ctx);
    }

    /// Write the session-owned highlight index — the pump's
    /// [`Deferred::SetMenuCurrent`](crate::view::Deferred::SetMenuCurrent) broker
    /// target. A box is never focused; the
    /// [`MenuSession`](crate::menu::MenuSession) drives its highlight through here.
    fn set_menu_current(&mut self, current: Option<usize>) {
        self.mv.current = current;
    }

    /// Expose the concrete box so the pump / tests can introspect its
    /// [`MenuViewState`] (the highlight cache the session drives). Mirrors the
    /// scroller/list broker downcast precedent.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }
}

impl MenuBox {
    /// Read the box's current highlight index (test/inspection hook for the
    /// session-driven highlight cache).
    pub fn current(&self) -> Option<usize> {
        self.mv.current
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::backend::{HeadlessBackend, Renderer};
    use crate::command::Command;
    use crate::event::{Key, KeyEvent};
    use crate::menu::alt;
    use crate::screen::Buffer;
    use crate::theme::Theme;

    fn render(b: &mut MenuBox, w: u16, h: u16) -> String {
        let theme = Theme::classic_blue();
        let (backend, screen) = HeadlessBackend::new(w, h);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = b.state().get_bounds();
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            b.draw(&mut dc);
        });
        screen.snapshot()
    }

    // -- menu_box_rect sizing -----------------------------------------------

    #[test]
    fn menu_box_rect_sizes_to_longest_item_and_item_count() {
        // Items:
        //   "~O~pen" (cstrlen 4) param "F3" (2)   -> 4 + 6 + 2 + 2 = 14
        //   "~S~ave" (cstrlen 4) no param         -> 4 + 6        = 10
        //   "~M~ore" submenu (cstrlen 4)          -> 4 + 6 + 3    = 13
        // w = max(14, 13, 10, 10) = 14; h = 2 + 3 items = 5.
        let menu = Menu::builder()
            .command_key("~O~pen", Command::OPEN, KeyEvent::from(Key::F(3)), "F3")
            .command("~S~ave", Command::SAVE)
            .submenu("~M~ore", alt('m'), |m| m.command("~X~", Command::QUIT))
            .build();
        // A generous bounds so the box is fully sized (not clamped).
        let r = menu_box_rect(Rect::new(0, 0, 40, 20), &menu);
        // width = 14 (the "Open ... F3" row wins).
        // BITE: dropping the param's "+ cstrlen + 2" would give w = max(10,13,10) =
        // 13; dropping the submenu's "+ 3" would give 12 from Open=14 -> still 14
        // here, so the param term is the discriminating one.
        assert_eq!(r.b.x - r.a.x, 14, "width = longest item (Open + F3 param)");
        // height = 2 (borders) + 3 items.
        assert_eq!(r.b.y - r.a.y, 5, "height = items + 2 borders");
    }

    #[test]
    fn menu_box_rect_counts_separators_in_height() {
        // 2 commands + 1 separator -> h = 2 + 3 = 5 (the separator takes a row).
        let menu = Menu::builder()
            .command("~A~", Command::OPEN)
            .separator()
            .command("~B~", Command::SAVE)
            .build();
        let r = menu_box_rect(Rect::new(0, 0, 40, 20), &menu);
        // BITE: a "separators don't take a row" bug would give h = 2 + 2 = 4.
        assert_eq!(r.b.y - r.a.y, 5, "separator counts toward height");
    }

    #[test]
    fn menu_box_rect_clamps_into_bounds() {
        // A wide menu inside a narrow bounds: w would be > bounds width, so the
        // box is pushed against the right edge (a.x = b.x - w), and likewise y.
        let menu = Menu::builder()
            .command("~A~", Command::OPEN)
            .command("~B~", Command::SAVE)
            .build();
        // w = 10 (both items short -> the w=10 floor), h = 4.
        // bounds is 8 wide: 0 + 10 < 8 is false -> a.x = b.x - 10 = 8 - 10 = -2.
        let r = menu_box_rect(Rect::new(0, 0, 8, 3), &menu);
        assert_eq!(r.a.x, -2, "narrow bounds clamps a.x = b.x - w");
        assert_eq!(r.b.x, 8);
        // h = 4, bounds is 3 tall: 0 + 4 < 3 false -> a.y = b.y - 4 = 3 - 4 = -1.
        assert_eq!(r.a.y, -1, "short bounds clamps a.y = b.y - h");
        assert_eq!(r.b.y, 3);
    }

    #[test]
    fn menu_box_rect_submenu_plus_three_is_discriminating() {
        // A submenu is the widest row, and no param item is longer, so the
        // submenu's "+ 3" (the ► marker column) actually decides the width.
        //   "~S~ettings" submenu (cstrlen 8) -> 8 + 6 + 3 = 17
        //   "~O~pen"     command (cstrlen 4) -> 4 + 6     = 10
        // w = max(17, 10, 10) = 17.
        let menu = Menu::builder()
            .submenu("~S~ettings", alt('s'), |m| m.command("~X~", Command::QUIT))
            .command("~O~pen", Command::OPEN)
            .build();
        let r = menu_box_rect(Rect::new(0, 0, 40, 20), &menu);
        // BITE: dropping the submenu's "+ 3" would give w = max(14, 10, 10) = 14.
        assert_eq!(
            r.b.x - r.a.x,
            17,
            "width = submenu name + 6 + 3 (the ► marker column)"
        );
    }

    // -- get_item_rect ------------------------------------------------------

    #[test]
    fn get_item_rect_counts_rows_from_one_including_separators() {
        // A separator before the asserted item proves separators still advance y.
        let menu = Menu::builder()
            .command("~A~", Command::OPEN) // idx 0 -> y 1
            .separator() // idx 1 -> y 2
            .command("~B~", Command::SAVE) // idx 2 -> y 3
            .build();
        let b = MenuBox::new(Rect::new(0, 0, 30, 10), menu);
        let size_x = b.mv.state.size.x;

        let r0 = b.get_item_rect(0);
        assert_eq!((r0.a.y, r0.b.y), (1, 2), "first item at row 1");
        assert_eq!((r0.a.x, r0.b.x), (2, size_x - 2), "x span is [2, size.x-2)");

        let r2 = b.get_item_rect(2);
        // BITE: a "skip separators" bug would put idx 2 at y = 2 (shifted up one).
        assert_eq!(
            (r2.a.y, r2.b.y),
            (3, 4),
            "third item at row 3 (separator still advanced y)"
        );
    }

    // -- handle_event delegation smoke -------------------------------------

    #[test]
    fn handle_event_posts_accelerator_command() {
        use crate::view::{Context, Deferred, Group};
        use std::collections::VecDeque;
        // A box whose item has an F3 accelerator; an F3 KeyDown posts cmOpen and
        // clears the event (proves the passive handler is wired in).
        let menu = Menu::builder()
            .command_key("~O~pen", Command::OPEN, KeyEvent::from(Key::F(3)), "F3")
            .build();
        let mut group = Group::new(Rect::new(0, 0, 30, 10));
        let id = group.insert(Box::new(MenuBox::new(Rect::new(2, 2, 20, 8), menu)));

        let mut out = VecDeque::new();
        let mut timers = crate::timer::TimerQueue::new();
        let mut deferred: Vec<Deferred> = vec![];
        let mut ev = Event::KeyDown(KeyEvent::from(Key::F(3)));
        {
            let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
            group.find_mut(id).unwrap().handle_event(&mut ev, &mut ctx);
        }
        assert!(ev.is_nothing(), "F3 accelerator consumed");
        assert!(
            out.iter()
                .any(|e| matches!(e, Event::Command(c) if *c == Command::OPEN)),
            "F3 posts cmOpen"
        );
    }

    // -- draw snapshot ------------------------------------------------------

    #[test]
    fn snapshot_box_frame_highlight_disabled_separator_param_submenu() {
        // A box exercising every draw path:
        //   idx 0 "~O~pen"  param "F3"        (a param/shortcut item)
        //   idx 1 "~S~ave"  disabled          (greyed)
        //   idx 2 separator                   (├──┤ divider)
        //   idx 3 "~M~ore"  submenu           (► marker), highlighted (current = 3)
        let menu = Menu::builder()
            .command_key("~O~pen", Command::OPEN, KeyEvent::from(Key::F(3)), "F3")
            .item(MenuItem::Command {
                name: "~S~ave".to_string(),
                command: Command::SAVE,
                key_code: None,
                param: None,
                help_ctx: crate::help::HelpCtx::NO_CONTEXT,
                disabled: true,
            })
            .separator()
            .submenu("~M~ore", alt('m'), |m| m.command("~X~", Command::QUIT))
            .build();
        let mut b = MenuBox::new(Rect::new(0, 0, 40, 20), menu);
        b.mv.current = Some(3); // highlight the submenu row
        let size = b.mv.state.size;
        insta::assert_snapshot!(render(&mut b, size.x as u16, size.y as u16));
    }

    #[test]
    fn draw_empty_menu_does_not_panic() {
        // An empty menu: the box draws top + bottom border (h = 2) with no item
        // rows. Cheapest guard against an index/iter edge case in draw.
        let mut b = MenuBox::new(Rect::new(0, 0, 12, 10), Menu::builder().build());
        let size = b.mv.state.size;
        let _ = render(&mut b, size.x as u16, size.y as u16); // completes, no panic.
    }
}
