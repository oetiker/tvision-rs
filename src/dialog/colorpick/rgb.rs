//! RGB surface — three R/G/B gauge bars + a #RRGGBB hex field.

use crate::color::{Color, Style};
use crate::dialog::colorpick::model::{ColorModel, color_to_display_rgb};
use crate::dialog::colorpick::{Surface, drag::ColorDragRegion};
use crate::event::{Event, Key};
use crate::theme::Role;
use crate::view::{Context, DrawCtx, Point, Rect};

pub(crate) struct RgbSurface {
    focus: u8,
    hex_buf: String,
}

impl RgbSurface {
    pub(crate) fn new() -> Self {
        RgbSurface {
            focus: 0,
            hex_buf: String::new(),
        }
    }
}

/// Row offsets within body for each field (body-relative y offsets).
const R_ROW: i32 = 1;
const G_ROW: i32 = 4;
const B_ROW: i32 = 7;
const HEX_ROW: i32 = 10;
const SWATCH_ROW: i32 = 12;

fn focus_row(focus: u8) -> i32 {
    match focus {
        0 => R_ROW,
        1 => G_ROW,
        2 => B_ROW,
        _ => HEX_ROW,
    }
}

impl Surface for RgbSurface {
    fn draw(&self, ctx: &mut DrawCtx, body: Rect, m: &ColorModel) {
        let (r, g, b) = color_to_display_rgb(m.color).unwrap_or((0, 0, 0));
        let normal = ctx.style(Role::ScrollerNormal);
        let focused = ctx.style(Role::ScrollerSelected);
        // bar starts after 6-char label: "R 255 " (label + space)
        let bar_x = body.a.x + 6;
        let bar_w = (body.b.x - bar_x).max(1);

        for (i, (label, val)) in [('R', r), ('G', g), ('B', b)].iter().enumerate() {
            let row_y = body.a.y + focus_row(i as u8);
            let style = if self.focus == i as u8 {
                focused
            } else {
                normal
            };
            // Clear the row
            ctx.fill(Rect::new(body.a.x, row_y, body.b.x, row_y + 1), ' ', style);
            // Label: "R 011"
            let label_str = format!("{} {:03}", label, val);
            ctx.put_str(body.a.x, row_y, &label_str, style);
            // Proportional bar
            let filled = (*val as i32 * bar_w / 255).min(bar_w);
            for fx in 0..filled {
                ctx.fill(
                    Rect::new(bar_x + fx, row_y, bar_x + fx + 1, row_y + 1),
                    '█',
                    style,
                );
            }
        }

        // Hex field
        let hex_y = body.a.y + HEX_ROW;
        let hex_style = if self.focus == 3 { focused } else { normal };
        ctx.fill(
            Rect::new(body.a.x, hex_y, body.b.x, hex_y + 1),
            ' ',
            hex_style,
        );
        let hex_display = if !self.hex_buf.is_empty() {
            format!("#{:<6}", self.hex_buf)
        } else {
            format!("#{:02X}{:02X}{:02X}", r, g, b)
        };
        ctx.put_str(body.a.x, hex_y, &hex_display, hex_style);

        // Swatch
        let sw_y = body.a.y + SWATCH_ROW;
        let swatch_style = Style::new(Color::Rgb(r, g, b), Color::Rgb(r, g, b));
        ctx.fill(
            Rect::new(body.a.x, sw_y, body.a.x + 4, sw_y + 1),
            ' ',
            swatch_style,
        );
        ctx.put_str(
            body.a.x + 5,
            sw_y,
            &format!("rgb({},{},{})", r, g, b),
            normal,
        );
    }

    fn handle_event(&mut self, ev: &mut Event, body: Rect, m: &mut ColorModel, _ctx: &mut Context) {
        match *ev {
            Event::KeyDown(ke) => {
                let (r, g, b) = color_to_display_rgb(m.color).unwrap_or((0, 0, 0));

                match ke.key {
                    // Hex field: typed hex digit (focus==3 only).
                    Key::Char(c) if self.focus == 3 && c.is_ascii_hexdigit() => {
                        let uc = c.to_ascii_uppercase();
                        self.hex_buf.push(uc);
                        if self.hex_buf.len() == 6 {
                            if let Ok(val) = u32::from_str_radix(&self.hex_buf, 16) {
                                let nr = ((val >> 16) & 0xFF) as u8;
                                let ng = ((val >> 8) & 0xFF) as u8;
                                let nb = (val & 0xFF) as u8;
                                m.set_rgb(nr, ng, nb);
                            }
                            self.hex_buf.clear();
                        }
                        ev.clear();
                    }
                    // Non-hex char in hex field: leave event uncleared.
                    Key::Char(_) if self.focus == 3 => {}
                    // Navigation keys (shared across all focus rows).
                    Key::Up => {
                        if self.focus > 0 {
                            self.focus -= 1;
                            self.hex_buf.clear();
                        }
                        ev.clear();
                    }
                    Key::Down => {
                        if self.focus < 3 {
                            self.focus += 1;
                            self.hex_buf.clear();
                        }
                        ev.clear();
                    }
                    Key::Right if self.focus < 3 => {
                        let chans = adjust_chan(r, g, b, self.focus, 1);
                        m.set_rgb(chans.0, chans.1, chans.2);
                        ev.clear();
                    }
                    Key::Left if self.focus < 3 => {
                        let chans = adjust_chan(r, g, b, self.focus, -1);
                        m.set_rgb(chans.0, chans.1, chans.2);
                        ev.clear();
                    }
                    Key::PageUp if self.focus < 3 => {
                        let chans = adjust_chan(r, g, b, self.focus, 16);
                        m.set_rgb(chans.0, chans.1, chans.2);
                        ev.clear();
                    }
                    Key::PageDown if self.focus < 3 => {
                        let chans = adjust_chan(r, g, b, self.focus, -16);
                        m.set_rgb(chans.0, chans.1, chans.2);
                        ev.clear();
                    }
                    _ => {}
                }
            }
            Event::MouseDown(me) => {
                if let Some(region) = self.drag_region_at(me.position, body) {
                    self.apply_drag(region, me.position, body, m);
                    ev.clear();
                }
            }
            _ => {}
        }
    }

    fn drag_region_at(&self, p: Point, body: Rect) -> Option<ColorDragRegion> {
        let bar_x = body.a.x + 6;
        for (chan, row_off) in [(0u8, R_ROW), (1, G_ROW), (2, B_ROW)] {
            let row_y = body.a.y + row_off;
            if p.y == row_y && p.x >= bar_x && p.x < body.b.x {
                return Some(ColorDragRegion::RgbBar(chan));
            }
        }
        None
    }

    fn apply_drag(&mut self, region: ColorDragRegion, p: Point, body: Rect, m: &mut ColorModel) {
        if let ColorDragRegion::RgbBar(chan) = region {
            let bar_x = body.a.x + 6;
            let bar_w = (body.b.x - bar_x).max(1);
            let x = (p.x - bar_x).clamp(0, bar_w - 1);
            let val = (x * 255 / (bar_w - 1).max(1)).clamp(0, 255) as u8;
            let (r, g, b) = color_to_display_rgb(m.color).unwrap_or((0, 0, 0));
            let (nr, ng, nb) = match chan {
                0 => (val, g, b),
                1 => (r, val, b),
                _ => (r, g, val),
            };
            m.set_rgb(nr, ng, nb);
        }
    }
}

/// Adjust one of the R/G/B channels by `delta`, saturating at [0, 255].
fn adjust_chan(r: u8, g: u8, b: u8, focus: u8, delta: i16) -> (u8, u8, u8) {
    fn adj(v: u8, d: i16) -> u8 {
        (v as i16 + d).clamp(0, 255) as u8
    }
    match focus {
        0 => (adj(r, delta), g, b),
        1 => (r, adj(g, delta), b),
        _ => (r, g, adj(b, delta)),
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
    fn ch(c: char) -> Event {
        Event::KeyDown(KeyEvent::new(Key::Char(c), KeyModifiers::default()))
    }
    const BODY: Rect = Rect {
        a: Point { x: 0, y: 1 },
        b: Point { x: 37, y: 18 },
    };

    #[test]
    fn right_arrow_increments_focused_channel() {
        let mut s = RgbSurface::new();
        let mut m = ColorModel::new(Color::Rgb(10, 20, 30));
        let mut ev = key(Key::Right);
        with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        assert_eq!(m.color, Color::Rgb(11, 20, 30));
    }

    #[test]
    fn right_arrow_saturates_at_255() {
        let mut s = RgbSurface::new();
        let mut m = ColorModel::new(Color::Rgb(255, 0, 0));
        let mut ev = key(Key::Right);
        with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        assert_eq!(m.color, Color::Rgb(255, 0, 0));
    }

    #[test]
    fn pgup_steps_focused_channel_by_16() {
        let mut s = RgbSurface::new();
        let mut m = ColorModel::new(Color::Rgb(0, 0, 0));
        let mut ev = key(Key::PageUp);
        with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        assert_eq!(m.color, Color::Rgb(16, 0, 0));
    }

    #[test]
    fn down_arrow_moves_focus_to_green() {
        let mut s = RgbSurface::new();
        let mut m = ColorModel::new(Color::Rgb(0, 0, 0));
        let mut ev = key(Key::Down);
        with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        let mut ev = key(Key::Right);
        with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        assert_eq!(m.color, Color::Rgb(0, 1, 0));
    }

    #[test]
    fn typing_six_hex_digits_commits() {
        let mut s = RgbSurface::new();
        let mut m = ColorModel::new(Color::Rgb(0, 0, 0));
        // Move focus to hex field (3 downs: R->G->B->Hex)
        for _ in 0..3 {
            let mut ev = key(Key::Down);
            with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        }
        for c in "1E90FF".chars() {
            let mut ev = ch(c);
            with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        }
        assert_eq!(m.color, Color::Rgb(0x1E, 0x90, 0xFF));
    }

    fn render_rgb(m: &ColorModel, focus: u8) -> String {
        use crate::backend::{HeadlessBackend, Renderer};
        use crate::screen::Buffer;
        use crate::theme::Theme;
        let theme = Theme::classic_blue();
        let (backend, screen) = HeadlessBackend::new(40, 18);
        let mut r = Renderer::new(Box::new(backend));
        let mut s = RgbSurface::new();
        s.focus = focus;
        r.render(|buf: &mut Buffer| {
            let bounds = Rect::new(0, 0, 40, 18);
            let mut dc = crate::view::DrawCtx::new(buf, &theme, bounds, bounds.a);
            <RgbSurface as crate::dialog::colorpick::Surface>::draw(&s, &mut dc, BODY, m);
        });
        screen.snapshot()
    }

    #[test]
    fn snapshot_rgb_at_dodger_blue() {
        // Dodger Blue: R=30 G=144 B=255, focus=0 (R focused)
        let m = ColorModel::new(Color::Rgb(30, 144, 255));
        insta::assert_snapshot!(render_rgb(&m, 0));
    }
}
