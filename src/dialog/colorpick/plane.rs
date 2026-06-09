//! Plane surface — hue strip + SV box (half-blocks).

use crate::color::{Color, Style};
use crate::dialog::colorpick::model::{ColorModel, Hsv, hsv_to_rgb};
use crate::dialog::colorpick::{Surface, drag::ColorDragRegion};
use crate::event::{Event, Key};
use crate::view::{Context, DrawCtx, Point, Rect};

pub(crate) struct PlaneSurface;

impl PlaneSurface {
    pub(crate) fn new() -> Self {
        PlaneSurface
    }
}

impl Surface for PlaneSurface {
    fn draw(&self, ctx: &mut DrawCtx, body: Rect, m: &ColorModel) {
        let height = (body.b.y - body.a.y).max(1);
        let box_x = body.a.x + 3;
        let bw = (body.b.x - box_x).max(1);
        let bh = height * 2; // half-block vertical levels

        // Hue strip (leftmost 2 cols)
        let hue_cursor_y = body.a.y + (m.hsv.h / 360.0 * height as f32) as i32;
        for y in body.a.y..body.b.y {
            let hue = (y - body.a.y) as f32 / height as f32 * 360.0;
            let (r, g, b) = hsv_to_rgb(Hsv {
                h: hue,
                s: 1.0,
                v: 1.0,
            });
            let col = Color::Rgb(r, g, b);
            let st = Style::new(col, col);
            ctx.fill(Rect::new(body.a.x, y, body.a.x + 2, y + 1), '█', st);
        }
        // Hue cursor marker
        let hue_cx = hue_cursor_y.clamp(body.a.y, body.b.y - 1);
        ctx.fill(
            Rect::new(body.a.x + 1, hue_cx, body.a.x + 2, hue_cx + 1),
            '◄',
            Style::new(Color::Rgb(255, 255, 255), Color::Rgb(0, 0, 0)),
        );

        // SV box with half-blocks
        let sv_cursor_cx = (box_x + (m.hsv.s * bw as f32) as i32).clamp(box_x, body.b.x - 1);
        let sv_cursor_cy =
            (body.a.y + ((1.0 - m.hsv.v) * (bh as f32 / 2.0)) as i32).clamp(body.a.y, body.b.y - 1);

        for cy in body.a.y..body.b.y {
            for cx in box_x..body.b.x {
                let sat = (cx - box_x) as f32 / bw as f32;
                let val_top = 1.0 - (((cy - body.a.y) * 2) as f32 / bh as f32);
                let val_bot = 1.0 - (((cy - body.a.y) * 2 + 1) as f32 / bh as f32);
                let (rt, gt, bt) = hsv_to_rgb(Hsv {
                    h: m.hsv.h,
                    s: sat,
                    v: val_top.clamp(0.0, 1.0),
                });
                let (rb, gb, bb) = hsv_to_rgb(Hsv {
                    h: m.hsv.h,
                    s: sat,
                    v: val_bot.clamp(0.0, 1.0),
                });
                let st = Style::new(Color::Rgb(rt, gt, bt), Color::Rgb(rb, gb, bb));
                ctx.fill(Rect::new(cx, cy, cx + 1, cy + 1), '▀', st);
            }
        }
        // SV cursor
        ctx.fill(
            Rect::new(
                sv_cursor_cx,
                sv_cursor_cy,
                sv_cursor_cx + 1,
                sv_cursor_cy + 1,
            ),
            '+',
            Style::new(Color::Rgb(255, 255, 255), Color::Rgb(0, 0, 0)),
        );
    }

    fn handle_event(&mut self, ev: &mut Event, body: Rect, m: &mut ColorModel, _ctx: &mut Context) {
        let height = (body.b.y - body.a.y).max(1);
        let box_x = body.a.x + 3;
        let bw = (body.b.x - box_x).max(1);
        let bh = height * 2;

        match *ev {
            Event::KeyDown(ke) => match ke.key {
                Key::Right => {
                    let new_s = (m.hsv.s + 1.0 / bw as f32).clamp(0.0, 1.0);
                    m.set_hsv(Hsv { s: new_s, ..m.hsv });
                    ev.clear();
                }
                Key::Left => {
                    let new_s = (m.hsv.s - 1.0 / bw as f32).clamp(0.0, 1.0);
                    m.set_hsv(Hsv { s: new_s, ..m.hsv });
                    ev.clear();
                }
                Key::Down => {
                    let new_v = (m.hsv.v - 1.0 / bh as f32).clamp(0.0, 1.0);
                    m.set_hsv(Hsv { v: new_v, ..m.hsv });
                    ev.clear();
                }
                Key::Up => {
                    let new_v = (m.hsv.v + 1.0 / bh as f32).clamp(0.0, 1.0);
                    m.set_hsv(Hsv { v: new_v, ..m.hsv });
                    ev.clear();
                }
                Key::Char(']') => {
                    let new_h = (m.hsv.h + 6.0).rem_euclid(360.0);
                    m.set_hsv(Hsv { h: new_h, ..m.hsv });
                    ev.clear();
                }
                Key::Char('[') => {
                    let new_h = (m.hsv.h - 6.0).rem_euclid(360.0);
                    m.set_hsv(Hsv { h: new_h, ..m.hsv });
                    ev.clear();
                }
                _ => {}
            },
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
        if p.y < body.a.y || p.y >= body.b.y {
            return None;
        }
        if p.x >= body.a.x && p.x < body.a.x + 2 {
            Some(ColorDragRegion::HueStrip)
        } else if p.x >= body.a.x + 3 && p.x < body.b.x {
            Some(ColorDragRegion::SvBox)
        } else {
            None
        }
    }

    fn apply_drag(&mut self, region: ColorDragRegion, p: Point, body: Rect, m: &mut ColorModel) {
        let height = (body.b.y - body.a.y).max(1) as f32;
        let box_x = body.a.x + 3;
        let bw = (body.b.x - box_x).max(1) as f32;
        match region {
            ColorDragRegion::HueStrip => {
                let hue = ((p.y - body.a.y) as f32 / height * 360.0).clamp(0.0, 359.9);
                m.set_hsv(Hsv { h: hue, ..m.hsv });
            }
            ColorDragRegion::SvBox => {
                let sat = ((p.x - box_x) as f32 / bw).clamp(0.0, 1.0);
                let val = (1.0 - (p.y - body.a.y) as f32 / height).clamp(0.0, 1.0);
                m.set_hsv(Hsv {
                    h: m.hsv.h,
                    s: sat,
                    v: val,
                });
            }
            _ => {}
        }
    }
}

#[cfg(test)]
#[allow(unused_imports)]
mod tests {
    use super::*;
    use crate::color::Color;
    use crate::dialog::colorpick::Surface;
    use crate::dialog::colorpick::model::{ColorModel, Hsv};
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
    fn right_arrow_increases_saturation() {
        let mut s = PlaneSurface::new();
        let mut m = ColorModel::new(Color::Rgb(128, 128, 128)); // mid gray, low sat
        let s0 = m.hsv.s;
        let mut ev = key(Key::Right);
        with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        assert!(m.hsv.s > s0, "saturation should rise");
        assert!(matches!(m.color, Color::Rgb(..)));
    }

    #[test]
    fn bracket_changes_hue() {
        let mut s = PlaneSurface::new();
        let mut m = ColorModel::new(Color::Rgb(255, 0, 0)); // hue 0
        let mut ev = ch(']');
        with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        assert!(m.hsv.h > 0.0, "] should advance hue");
    }

    #[test]
    fn down_arrow_decreases_value_without_scrambling_hue() {
        let mut s = PlaneSurface::new();
        let mut m = ColorModel::new(Color::Rgb(255, 165, 0)); // orange
        let h0 = m.hsv.h;
        for _ in 0..40 {
            let mut ev = key(Key::Down);
            with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        }
        assert!(
            (m.hsv.h - h0).abs() <= 0.5,
            "hue retained as value drops to 0"
        );
    }

    // Small body: 2-col hue strip + 3-col SV box = 5 cols wide, 4 rows tall.
    // body.a.x=0, body.b.x=5 → box_x=3, bw=2, bh=8.
    // Keeping the area small stays well under the 63-style snapshot limit.
    const SNAP_BODY: Rect = Rect {
        a: Point { x: 0, y: 0 },
        b: Point { x: 5, y: 4 },
    };

    fn render_plane(m: &ColorModel) -> String {
        use crate::backend::{HeadlessBackend, Renderer};
        use crate::screen::Buffer;
        use crate::theme::Theme;
        let theme = Theme::classic_blue();
        // Canvas is exactly snap body size.
        let (backend, screen) = HeadlessBackend::new(5, 4);
        let mut r = Renderer::new(Box::new(backend));
        let s = PlaneSurface::new();
        r.render(|buf: &mut Buffer| {
            let bounds = Rect::new(0, 0, 5, 4);
            let mut dc = crate::view::DrawCtx::new(buf, &theme, bounds, bounds.a);
            <PlaneSurface as crate::dialog::colorpick::Surface>::draw(&s, &mut dc, SNAP_BODY, m);
        });
        screen.snapshot()
    }

    #[test]
    fn snapshot_plane_at_orange() {
        let m = ColorModel::new(Color::Rgb(255, 165, 0)); // orange
        insta::assert_snapshot!(render_plane(&m));
    }
}
