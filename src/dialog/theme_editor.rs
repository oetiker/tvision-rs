//! `ThemeEditorBody` — a scrollable role-list widget for the theme editor.
//!
//! This is the content pane of the theme-editor dialog: it shows all 75
//! [`Role`]s with their current foreground / background colors and a "AaBb"
//! preview, lets the user navigate with the keyboard, and requests a
//! [`ColorPicker`](crate::dialog::ColorPicker) dialog for the selected role via
//! the [`Deferred::OpenColorDialogForRole`](crate::view::Deferred) channel — a
//! leaf view cannot run a modal dialog inline, so it routes the request to the
//! event-loop owner.
//!
//! This is an rstv-original widget with no Turbo Vision counterpart.

use crate::{
    color::{Color, Style},
    command::Command,
    event::{Event, Key},
    theme::{ALL, ROLE_COUNT, Role, Theme},
    view::{Context, DrawCtx, Rect, View, ViewState},
};

/// Number of role rows visible at once inside the bounds (no header row counted
/// here — the header is row 0 in local coords, roles start at row 1).
const VISIBLE_ROWS: usize = 17;

/// The scrollable role-list body of the theme-editor dialog.
///
/// Draws a header row followed by up to [`VISIBLE_ROWS`] role rows. Each role
/// row shows:
/// - a `>` cursor indicator
/// - the role name (up to 16 chars)
/// - a 3-cell foreground swatch (solid-block chars in the role's fg color)
/// - a 3-cell background swatch (spaces in the role's bg color)
/// - a 4-char `"AaBb"` preview drawn in the role's full style
///
/// Navigation: Up/Down/PgUp/PgDn/Home/End. Commands `THEME_EDIT_FG` /
/// `THEME_EDIT_BG` (and hotkeys `f`/`b`) open a color picker.
pub struct ThemeEditorBody {
    state: ViewState,
    /// Working copy of the theme being edited.
    theme: Theme,
    /// Index of the selected role in [`ALL`].
    cursor: usize,
    /// Index of the first visible role (scroll offset).
    scroll_top: usize,
}

impl ThemeEditorBody {
    /// Construct with `bounds` (in the dialog's local coordinate system) and
    /// `initial` as the working copy of the theme.
    pub fn new(bounds: Rect, initial: Theme) -> Self {
        Self {
            state: ViewState::new(bounds),
            theme: initial,
            cursor: 0,
            scroll_top: 0,
        }
    }

    /// Borrow the working theme (read by the `ThemeEdit` completion in
    /// `apply_modal_completion` to harvest the user's edits).
    pub fn working_theme(&self) -> &Theme {
        &self.theme
    }

    /// Update the style for `role` in the working theme (called by the
    /// `ThemeColorPick` completion in `apply_modal_completion`).
    pub fn set_role_style(&mut self, role: Role, style: Style) {
        self.theme.set_style(role, style);
    }

    /// Adjust `scroll_top` so the cursor is visible.
    fn ensure_cursor_visible(&mut self) {
        if self.cursor < self.scroll_top {
            self.scroll_top = self.cursor;
        } else if self.cursor >= self.scroll_top + VISIBLE_ROWS {
            self.scroll_top = self.cursor + 1 - VISIBLE_ROWS;
        }
    }

    /// Queue an `OpenColorDialogForRole` deferred for the current cursor row.
    /// `fg` selects foreground vs. background.
    fn request_edit(&self, fg: bool, ctx: &mut Context) {
        if let Some(id) = self.state.id() {
            let role = ALL[self.cursor];
            let style = self.theme.style(role);
            let current = if fg { style.fg } else { style.bg };
            ctx.open_color_dialog_for_role(id, role, fg, current);
        }
    }
}

impl View for ThemeEditorBody {
    fn state(&self) -> &ViewState {
        &self.state
    }

    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.state
    }

    /// Downcast support for the `ThemeColorPick` / `ThemeEdit` completions.
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    fn draw(&mut self, ctx: &mut DrawCtx) {
        let bounds = self.state.get_bounds();
        let width = bounds.b.x - bounds.a.x;

        // Row 0: header line.
        let header_style = ctx.style(crate::theme::Role::Focused);
        // Fill the header row first.
        ctx.fill(Rect::new(0, 0, width, 1), ' ', header_style);
        ctx.put_str(0, 0, " Role             Fg  Bg  Preview", header_style);

        // Role rows: indices scroll_top .. scroll_top+VISIBLE_ROWS.
        let normal_style = ctx.style(crate::theme::Role::Normal);
        let focused_style = ctx.style(crate::theme::Role::Focused);

        for row_idx in 0..VISIBLE_ROWS {
            let list_idx = self.scroll_top + row_idx;
            let y = (row_idx + 1) as i32; // +1 to skip header

            if list_idx >= ROLE_COUNT {
                // Blank out remaining rows.
                ctx.fill(Rect::new(0, y, width, y + 1), ' ', normal_style);
                continue;
            }

            let role = ALL[list_idx];
            let role_style = self.theme.style(role);
            let row_bg_style = if list_idx == self.cursor {
                focused_style
            } else {
                normal_style
            };

            // Fill row background.
            ctx.fill(Rect::new(0, y, width, y + 1), ' ', row_bg_style);

            // Cursor indicator.
            let indicator = if list_idx == self.cursor { '>' } else { ' ' };
            ctx.put_char(0, y, indicator, row_bg_style);

            // Role name: 16 chars, left-aligned, padded to exactly 16 columns.
            let name = role.name();
            let padded = format!("{:<16}", name);
            ctx.put_str(1, y, &padded, row_bg_style);

            // Column 18: foreground swatch — 3 solid blocks in the role's fg color.
            let fg_swatch_style = Style::new(role_style.fg, role_style.fg);
            ctx.put_str(18, y, "███", fg_swatch_style);

            // Column 22: background swatch — 3 spaces in the role's bg color.
            let bg_swatch_style = Style::new(Color::Default, role_style.bg);
            ctx.put_str(22, y, "   ", bg_swatch_style);

            // Column 26: preview — "AaBb" in the actual role style.
            ctx.put_str(26, y, "AaBb", role_style);
        }
    }

    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        match ev {
            Event::KeyDown(k) => match k.key {
                Key::Up => {
                    if self.cursor > 0 {
                        self.cursor -= 1;
                        self.ensure_cursor_visible();
                    }
                    ev.clear();
                }
                Key::Down => {
                    if self.cursor + 1 < ROLE_COUNT {
                        self.cursor += 1;
                        self.ensure_cursor_visible();
                    }
                    ev.clear();
                }
                Key::PageUp => {
                    self.cursor = self.cursor.saturating_sub(VISIBLE_ROWS);
                    self.ensure_cursor_visible();
                    ev.clear();
                }
                Key::PageDown => {
                    self.cursor = (self.cursor + VISIBLE_ROWS).min(ROLE_COUNT - 1);
                    self.ensure_cursor_visible();
                    ev.clear();
                }
                Key::Home => {
                    self.cursor = 0;
                    self.scroll_top = 0;
                    ev.clear();
                }
                Key::End => {
                    self.cursor = ROLE_COUNT - 1;
                    self.ensure_cursor_visible();
                    ev.clear();
                }
                Key::Char('f') | Key::Char('F') => {
                    self.request_edit(true, ctx);
                    ev.clear();
                }
                Key::Char('b') | Key::Char('B') => {
                    self.request_edit(false, ctx);
                    ev.clear();
                }
                _ => {}
            },
            Event::Command(cmd) => {
                if *cmd == Command::THEME_EDIT_FG {
                    self.request_edit(true, ctx);
                    ev.clear();
                } else if *cmd == Command::THEME_EDIT_BG {
                    self.request_edit(false, ctx);
                    ev.clear();
                }
            }
            Event::MouseDown(m) if m.buttons.left => {
                // row 0 is the header; skip it.
                if m.position.y > 0 {
                    let row = (m.position.y - 1) as usize;
                    let idx = self.scroll_top + row;
                    if idx < ROLE_COUNT {
                        self.cursor = idx;
                        self.ensure_cursor_visible();
                    }
                }
                ev.clear();
            }
            _ => {}
        }
    }
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn set_role_style_updates_working_theme() {
        let mut te = ThemeEditorBody::new(Rect::new(0, 0, 62, 18), Theme::classic_blue());
        let new_style = Style::new(Color::Bios(0xF), Color::Bios(0x4));
        te.set_role_style(Role::Background, new_style);
        assert_eq!(te.working_theme().style(Role::Background), new_style);
    }

    #[test]
    fn navigation_clamps_at_bounds() {
        let mut te = ThemeEditorBody::new(Rect::new(0, 0, 62, 18), Theme::classic_blue());
        // Up from 0 should stay at 0.
        te.cursor = 0;
        if te.cursor > 0 {
            te.cursor -= 1;
        }
        assert_eq!(te.cursor, 0);
        // Down past last should clamp.
        te.cursor = ROLE_COUNT - 1;
        te.cursor = (te.cursor + 1).min(ROLE_COUNT - 1);
        assert_eq!(te.cursor, ROLE_COUNT - 1);
    }

    #[test]
    fn ensure_cursor_visible_scrolls_down() {
        let mut te = ThemeEditorBody::new(Rect::new(0, 0, 62, 18), Theme::classic_blue());
        te.scroll_top = 0;
        te.cursor = VISIBLE_ROWS + 5;
        te.ensure_cursor_visible();
        assert!(te.cursor < te.scroll_top + VISIBLE_ROWS);
        assert!(te.cursor >= te.scroll_top);
    }

    #[test]
    fn ensure_cursor_visible_scrolls_up() {
        let mut te = ThemeEditorBody::new(Rect::new(0, 0, 62, 18), Theme::classic_blue());
        te.scroll_top = 10;
        te.cursor = 3;
        te.ensure_cursor_visible();
        assert_eq!(te.scroll_top, 3);
    }
}
