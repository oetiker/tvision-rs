//! Plane surface — hue strip + SV box (half-blocks).

use crate::color::{Color, Style};
use crate::dialog::colorpick::model::{ColorModel, Hsv, hsv_to_rgb};
use crate::dialog::colorpick::{Surface, drag::ColorDragRegion};
use crate::event::{Event, Key};
use crate::view::{Context, DrawCtx, Point, Rect};

// Hue strip is 4 columns wide; SV box starts at box_x = body.a.x + HUE_COLS + 1
// (one gap column between strip and box for visual separation).
const HUE_COLS: i32 = 4;
const BOX_OFFSET: i32 = HUE_COLS + 1; // 5

pub(crate) struct PlaneSurface;

impl PlaneSurface {
    pub(crate) fn new() -> Self {
        PlaneSurface
    }
}

impl Surface for PlaneSurface {
    fn draw(&self, ctx: &mut DrawCtx, body: Rect, m: &ColorModel) {
        let height = (body.b.y - body.a.y).max(1);
        let box_x = body.a.x + BOX_OFFSET;
        let bw = (body.b.x - box_x).max(1);
        let bh = height * 2; // half-block vertical levels

        // Hue strip — raster half-block layout: hue increases left→right across
        // all 4 cols, wraps to the next row. Each physical row covers two sub-rows
        // via ▀ (fg=top, bg=bottom), giving 8× more hue steps than a single column.
        let total_hue = height * 2 * HUE_COLS;
        for y in body.a.y..body.b.y {
            for cx in 0..HUE_COLS {
                let row = y - body.a.y;
                let idx_t = (row * 2) * HUE_COLS + cx;
                let idx_b = (row * 2 + 1) * HUE_COLS + cx;
                let hue_t = idx_t as f32 / total_hue as f32 * 360.0;
                let hue_b = idx_b as f32 / total_hue as f32 * 360.0;
                let (rt, gt, bt) = hsv_to_rgb(Hsv {
                    h: hue_t,
                    s: 1.0,
                    v: 1.0,
                });
                let (rb, gb, bb) = hsv_to_rgb(Hsv {
                    h: hue_b,
                    s: 1.0,
                    v: 1.0,
                });
                ctx.fill(
                    Rect::new(body.a.x + cx, y, body.a.x + cx + 1, y + 1),
                    '▀',
                    Style::new(Color::Rgb(rt, gt, bt), Color::Rgb(rb, gb, bb)),
                );
            }
        }
        // Hue cursor: ▲/▼ at the sub-row matching the current hue (uniform with
        // the SV box cursor style). Fg = hue color; bg = black or white by luminance.
        let hue_idx = (m.hsv.h / 360.0 * total_hue as f32) as i32;
        let hue_sub = hue_idx / HUE_COLS; // virtual row in the doubled grid
        let hue_col = hue_idx % HUE_COLS;
        let hue_phy = (body.a.y + hue_sub / 2).clamp(body.a.y, body.b.y - 1);
        let hue_is_bot = (hue_sub % 2) == 1;
        let cursor_hue = hue_idx as f32 / total_hue as f32 * 360.0;
        let (hcr, hcg, hcb) = hsv_to_rgb(Hsv {
            h: cursor_hue,
            s: 1.0,
            v: 1.0,
        });
        let hue_lum = 0.299 * hcr as f32 + 0.587 * hcg as f32 + 0.114 * hcb as f32;
        let hue_bg = if hue_lum > 128.0 {
            Color::Rgb(0, 0, 0)
        } else {
            Color::Rgb(255, 255, 255)
        };
        ctx.fill(
            Rect::new(
                body.a.x + hue_col,
                hue_phy,
                body.a.x + hue_col + 1,
                hue_phy + 1,
            ),
            if hue_is_bot { '▼' } else { '▲' },
            Style::new(Color::Rgb(hcr, hcg, hcb), hue_bg),
        );

        // SV box — half-block cells, each physical row covers two value levels.
        let sv_cx = (box_x + (m.hsv.s * bw as f32) as i32).clamp(box_x, body.b.x - 1);
        let sv_cy =
            (body.a.y + ((1.0 - m.hsv.v) * height as f32) as i32).clamp(body.a.y, body.b.y - 1);

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
                ctx.fill(
                    Rect::new(cx, cy, cx + 1, cy + 1),
                    '▀',
                    Style::new(Color::Rgb(rt, gt, bt), Color::Rgb(rb, gb, bb)),
                );
            }
        }

        // SV cursor — ▲ when pointing at the top sub-cell, ▼ at the bottom
        // sub-cell. The triangle's fg uses the actual gradient color at the
        // cursor position; bg is black or white chosen by luminance so the cursor
        // is always visible (a fully-gradient cursor vanishes at s≈0 where the
        // two sub-colors converge).
        let sub_val = (1.0 - m.hsv.v) * bh as f32;
        let sub_row = (sub_val as i32).clamp(0, bh - 1);
        let is_bottom_sub = (sub_row % 2) == 1;

        let sat_c = (sv_cx - box_x) as f32 / bw as f32;
        let val_top_c = 1.0 - (((sv_cy - body.a.y) * 2) as f32 / bh as f32);
        let val_bot_c = 1.0 - (((sv_cy - body.a.y) * 2 + 1) as f32 / bh as f32);
        let (rt, gt, bt) = hsv_to_rgb(Hsv {
            h: m.hsv.h,
            s: sat_c,
            v: val_top_c.clamp(0.0, 1.0),
        });
        let (rb, gb, bb) = hsv_to_rgb(Hsv {
            h: m.hsv.h,
            s: sat_c,
            v: val_bot_c.clamp(0.0, 1.0),
        });

        let (cursor_ch, (cr, cg, cb)) = if is_bottom_sub {
            ('▼', (rb, gb, bb)) // cursor in lower sub-cell → down-pointing triangle
        } else {
            ('▲', (rt, gt, bt)) // cursor in upper sub-cell → up-pointing triangle
        };
        // Pick bg that guarantees contrast: dark on bright, light on dark.
        let lum = 0.299 * cr as f32 + 0.587 * cg as f32 + 0.114 * cb as f32;
        let bg = if lum > 128.0 {
            Color::Rgb(0, 0, 0)
        } else {
            Color::Rgb(255, 255, 255)
        };
        ctx.fill(
            Rect::new(sv_cx, sv_cy, sv_cx + 1, sv_cy + 1),
            cursor_ch,
            Style::new(Color::Rgb(cr, cg, cb), bg),
        );
    }

    fn handle_event(&mut self, ev: &mut Event, body: Rect, m: &mut ColorModel, _ctx: &mut Context) {
        let height = (body.b.y - body.a.y).max(1);
        let box_x = body.a.x + BOX_OFFSET;
        let bw = (body.b.x - box_x).max(1);
        let bh = height * 2;

        match *ev {
            Event::KeyDown(ke) => match ke.key {
                // Shift+Up/Down: step hue by one sub-row (360/bh degrees).
                // Symmetric with how Up/Down steps value by 1/bh.
                Key::Up if ke.modifiers.shift => {
                    let new_h = (m.hsv.h - 360.0 / bh as f32).rem_euclid(360.0);
                    m.set_hsv(Hsv { h: new_h, ..m.hsv });
                    ev.clear();
                }
                Key::Down if ke.modifiers.shift => {
                    let new_h = (m.hsv.h + 360.0 / bh as f32).rem_euclid(360.0);
                    m.set_hsv(Hsv { h: new_h, ..m.hsv });
                    ev.clear();
                }
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
        let box_x = body.a.x + BOX_OFFSET;
        if p.x >= body.a.x && p.x < body.a.x + HUE_COLS {
            Some(ColorDragRegion::HueStrip)
        } else if p.x >= box_x && p.x < body.b.x {
            Some(ColorDragRegion::SvBox)
        } else {
            None
        }
    }

    fn apply_drag(&mut self, region: ColorDragRegion, p: Point, body: Rect, m: &mut ColorModel) {
        let height = (body.b.y - body.a.y).max(1);
        let box_x = body.a.x + BOX_OFFSET;
        let bw = (body.b.x - box_x).max(1) as f32;
        match region {
            ColorDragRegion::HueStrip => {
                let col = (p.x - body.a.x).clamp(0, HUE_COLS - 1);
                let row = (p.y - body.a.y).clamp(0, height - 1);
                let total = height * 2 * HUE_COLS;
                // Map physical row to the top sub-row of the clicked cell.
                let idx = row * 2 * HUE_COLS + col;
                let hue = (idx as f32 / total as f32 * 360.0).clamp(0.0, 359.9);
                m.set_hsv(Hsv { h: hue, ..m.hsv });
            }
            ColorDragRegion::SvBox => {
                let sat = ((p.x - box_x) as f32 / bw).clamp(0.0, 1.0);
                let val = (1.0 - (p.y - body.a.y) as f32 / height as f32).clamp(0.0, 1.0);
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

    fn shift_key(k: Key) -> Event {
        Event::KeyDown(KeyEvent::new(
            k,
            KeyModifiers {
                shift: true,
                ..Default::default()
            },
        ))
    }

    #[test]
    fn shift_down_increases_hue() {
        let mut s = PlaneSurface::new();
        let mut m = ColorModel::new(Color::Rgb(255, 0, 0)); // hue 0
        let h0 = m.hsv.h;
        let mut ev = shift_key(Key::Down);
        with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        assert!(m.hsv.h > h0, "Shift+Down should advance hue");
        assert!(matches!(ev, Event::Nothing), "event consumed");
    }

    #[test]
    fn shift_up_decreases_hue_with_wraparound() {
        let mut s = PlaneSurface::new();
        let mut m = ColorModel::new(Color::Rgb(255, 0, 0)); // hue 0
        let mut ev = shift_key(Key::Up);
        with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        // Wrap: 0 - step → near 360
        assert!(m.hsv.h > 300.0, "Shift+Up from hue 0 wraps to near 360");
    }

    #[test]
    fn shift_up_down_does_not_change_saturation_or_value() {
        let mut s = PlaneSurface::new();
        let mut m = ColorModel::new(Color::Rgb(200, 100, 50));
        let s0 = m.hsv.s;
        let v0 = m.hsv.v;
        let mut ev = shift_key(Key::Down);
        with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        assert!((m.hsv.s - s0).abs() < 0.001, "saturation unchanged");
        assert!((m.hsv.v - v0).abs() < 0.001, "value unchanged");
    }

    #[test]
    fn plain_up_down_does_not_change_hue() {
        let mut s = PlaneSurface::new();
        let mut m = ColorModel::new(Color::Rgb(255, 165, 0)); // orange, hue ≈ 38.8
        let h0 = m.hsv.h;
        let mut ev = key(Key::Down);
        with_ctx(|ctx| s.handle_event(&mut ev, BODY, &mut m, ctx));
        assert!((m.hsv.h - h0).abs() < 0.5, "plain Down must not change hue");
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

    // Snapshot body: 4-col hue strip + 1-col gap + 4-col SV box = 9 cols wide, 4 rows tall.
    // body.a.x=0 → box_x=5, bw=4, bh=8.
    const SNAP_BODY: Rect = Rect {
        a: Point { x: 0, y: 0 },
        b: Point { x: 9, y: 4 },
    };

    fn render_plane(m: &ColorModel) -> String {
        use crate::backend::{HeadlessBackend, Renderer};
        use crate::screen::Buffer;
        use crate::theme::Theme;
        let theme = Theme::classic_blue();
        let (backend, screen) = HeadlessBackend::new(9, 4);
        let mut r = Renderer::new(Box::new(backend));
        let s = PlaneSurface::new();
        r.render(|buf: &mut Buffer| {
            let bounds = Rect::new(0, 0, 9, 4);
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
