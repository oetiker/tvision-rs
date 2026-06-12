//! Xterm-256 surface — a true 16×16 grid of the 256-color palette.

// Some drag-related imports are not used by this surface.
#![allow(unused_imports)]

use crate::backend::{rgb_to_xterm256, xterm256_to_rgb};
use crate::color::{Color, Style};
use crate::dialog::colorpick::model::{ColorModel, color_to_display_rgb};
use crate::dialog::colorpick::{Surface, drag::ColorDragRegion};
use crate::event::{Event, Key};
use crate::view::{Context, DrawCtx, Point, Rect};

/// Xterm-256 surface: a 16×16 grid where each cell displays its palette color.
/// Each cell is 2 columns wide and 1 row tall. The cursor marks the selected
/// index with a `◘` character.
pub(crate) struct Xterm256Surface {
    pub(crate) cursor: u8,
}

impl Xterm256Surface {
    pub(crate) fn new(m: &ColorModel) -> Self {
        let cursor = match m.color {
            Color::Indexed(n) => n,
            other => {
                let (r, g, b) = color_to_display_rgb(other).unwrap_or((0, 0, 0));
                rgb_to_xterm256(r, g, b)
            }
        };
        Xterm256Surface { cursor }
    }
}

impl Surface for Xterm256Surface {
    fn draw(&self, ctx: &mut DrawCtx, body: Rect, _m: &ColorModel) {
        for row in 0..16i32 {
            let y = body.a.y + row;
            if y >= body.b.y {
                break;
            }
            for col in 0..16i32 {
                let x = body.a.x + col * 2;
                if x + 2 > body.b.x {
                    break;
                }
                let idx = (row * 16 + col) as u8;
                let (r, g, b) = xterm256_to_rgb(idx);
                let col_color = Color::Rgb(r, g, b);
                let style = Style::new(col_color, col_color);
                ctx.fill(Rect::new(x, y, x + 2, y + 1), '\u{2588}', style);
                // Cursor marker: overlay right char of cursor cell with a small diamond.
                if idx == self.cursor {
                    let cx = x + 1;
                    ctx.fill(
                        Rect::new(cx, y, cx + 1, y + 1),
                        '\u{25D8}',
                        Style::new(Color::Rgb(255, 255, 255), col_color),
                    );
                }
            }
        }
    }

    fn handle_event(&mut self, ev: &mut Event, body: Rect, m: &mut ColorModel, _ctx: &mut Context) {
        match *ev {
            Event::KeyDown(ke) => {
                let col = self.cursor % 16;
                let row = self.cursor / 16;
                let (new_col, new_row) = match ke.key {
                    Key::Left => (col.saturating_sub(1), row),
                    Key::Right => (if col < 15 { col + 1 } else { col }, row),
                    Key::Up => (col, row.saturating_sub(1)),
                    Key::Down => (col, if row < 15 { row + 1 } else { row }),
                    _ => return, // unhandled — leave event intact
                };
                self.cursor = new_row * 16 + new_col;
                m.set_indexed(self.cursor);
                ev.clear();
            }
            Event::MouseDown(me) => {
                let col = (me.position.x - body.a.x) / 2;
                let row = me.position.y - body.a.y;
                if (0..16).contains(&col) && (0..16).contains(&row) {
                    self.cursor = (row * 16 + col) as u8;
                    m.set_indexed(self.cursor);
                    ev.clear();
                }
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;
    use crate::dialog::colorpick::Surface;
    use crate::dialog::colorpick::model::ColorModel;
    use crate::event::{Event, Key, KeyEvent, KeyModifiers};
    use crate::timer::TimerQueue;
    use crate::view::{Context, Deferred, Point, Rect};
    use std::collections::VecDeque;

    fn with_ctx<R>(f: impl FnOnce(&mut Context) -> R) -> R {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
        f(&mut ctx)
    }

    fn key(k: Key) -> Event {
        Event::KeyDown(KeyEvent::new(k, KeyModifiers::default()))
    }

    const BODY: Rect = Rect {
        a: Point { x: 0, y: 1 },
        b: Point { x: 32, y: 17 },
    };

    #[test]
    fn new_seeds_cursor_from_indexed() {
        let s = Xterm256Surface::new(&ColorModel::new(Color::Indexed(33)));
        assert_eq!(s.cursor, 33);
    }

    #[test]
    fn right_moves_cursor_and_sets_indexed() {
        let mut s = Xterm256Surface::new(&ColorModel::new(Color::Indexed(0)));
        let mut m = ColorModel::new(Color::Indexed(0));
        let mut ev = key(Key::Right);
        with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        assert_eq!(s.cursor, 1);
        assert_eq!(m.color, Color::Indexed(1));
    }

    #[test]
    fn down_moves_cursor_one_row() {
        let mut s = Xterm256Surface::new(&ColorModel::new(Color::Indexed(0)));
        let mut m = ColorModel::new(Color::Indexed(0));
        let mut ev = key(Key::Down);
        with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        assert_eq!(s.cursor, 16); // one row down = +16
        assert_eq!(m.color, Color::Indexed(16));
    }

    #[test]
    fn right_at_255_clamps() {
        let mut s = Xterm256Surface::new(&ColorModel::new(Color::Indexed(255)));
        let mut m = ColorModel::new(Color::Indexed(255));
        let mut ev = key(Key::Right);
        with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        assert_eq!(s.cursor, 255);
    }

    fn render_xterm(s: &Xterm256Surface) -> String {
        use crate::backend::{HeadlessBackend, Renderer};
        use crate::screen::Buffer;
        use crate::theme::Theme;
        let theme = Theme::classic_blue();
        // Small grid: 4 cols × 4 rows = 16 cells (indices 0..=3, 16..=19, 32..=35, 48..=51)
        // body.a = (0,1) so the grid starts on row 1; canvas height = 5 (row 0 is empty).
        let (backend, screen) = HeadlessBackend::new(8, 5);
        let mut r = Renderer::new(Box::new(backend));
        let snap_body = Rect {
            a: Point { x: 0, y: 1 },
            b: Point { x: 8, y: 5 },
        };
        let m = ColorModel::new(Color::Indexed(0));
        r.render(|buf: &mut Buffer| {
            let bounds = Rect::new(0, 0, 8, 5);
            let mut dc = crate::view::DrawCtx::new(buf, &theme, bounds, bounds.a);
            <Xterm256Surface as crate::dialog::colorpick::Surface>::draw(s, &mut dc, snap_body, &m);
        });
        screen.snapshot()
    }

    #[test]
    fn snapshot_xterm256_cursor_at_33() {
        let s = Xterm256Surface::new(&ColorModel::new(Color::Indexed(33)));
        // cursor=33 is col=1 row=2 (0-indexed). In a 4-col 4-row body, row 2 col 1 is index 33.
        insta::assert_snapshot!(render_xterm(&s));
    }
}
