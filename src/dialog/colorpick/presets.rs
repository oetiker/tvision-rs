//! Presets surface — a scrolling list of {name, Color}: Default + 16 BIOS + 12 Rgb.

// Some palette/import items are not used by this surface.
#![allow(dead_code, unused_imports)]

use crate::color::{Color, Style};
use crate::dialog::colorpick::model::{ColorModel, color_to_display_rgb};
use crate::dialog::colorpick::{Surface, drag::ColorDragRegion};
use crate::event::{Event, Key, ctrl_to_arrow};
use crate::theme::Role;
use crate::view::{Context, DrawCtx, Point, Rect};

pub(crate) const PRESETS: &[(&str, Color)] = &[
    ("Default", Color::Default),
    ("Black", Color::Bios(0)),
    ("Blue", Color::Bios(1)),
    ("Green", Color::Bios(2)),
    ("Cyan", Color::Bios(3)),
    ("Red", Color::Bios(4)),
    ("Magenta", Color::Bios(5)),
    ("Brown", Color::Bios(6)),
    ("Light Gray", Color::Bios(7)),
    ("Dark Gray", Color::Bios(8)),
    ("Light Blue", Color::Bios(9)),
    ("Light Green", Color::Bios(10)),
    ("Light Cyan", Color::Bios(11)),
    ("Light Red", Color::Bios(12)),
    ("Light Magenta", Color::Bios(13)),
    ("Yellow", Color::Bios(14)),
    ("White", Color::Bios(15)),
    ("Orange", Color::Rgb(255, 165, 0)),
    ("Gold", Color::Rgb(255, 215, 0)),
    ("Pink", Color::Rgb(255, 192, 203)),
    ("Coral", Color::Rgb(255, 127, 80)),
    ("Purple", Color::Rgb(128, 0, 128)),
    ("Teal", Color::Rgb(0, 128, 128)),
    ("Olive", Color::Rgb(128, 128, 0)),
    ("Navy", Color::Rgb(0, 0, 128)),
    ("Maroon", Color::Rgb(128, 0, 0)),
    ("Lime", Color::Rgb(0, 255, 0)),
    ("Aqua", Color::Rgb(0, 255, 255)),
    ("Silver", Color::Rgb(192, 192, 192)),
];

pub(crate) struct PresetsSurface {
    pub(crate) selected: usize,
    top: usize,
}

impl PresetsSurface {
    pub(crate) fn new(m: &ColorModel) -> Self {
        let selected = PRESETS.iter().position(|&(_, c)| c == m.color).unwrap_or(0);
        PresetsSurface { selected, top: 0 }
    }

    fn scroll_into_view(&mut self, rows: usize) {
        if self.selected < self.top {
            self.top = self.selected;
        } else if rows > 0 && self.selected >= self.top + rows {
            self.top = self.selected + 1 - rows;
        }
    }
}

impl Surface for PresetsSurface {
    fn draw(&self, ctx: &mut DrawCtx, body: Rect, _m: &ColorModel) {
        let rows = (body.b.y - body.a.y) as usize;
        let normal = ctx.style(Role::ScrollerNormal);
        let selected_style = ctx.style(Role::ScrollerSelected);
        for i in 0..rows {
            let idx = self.top + i;
            if idx >= PRESETS.len() {
                break;
            }
            let y = body.a.y + i as i32;
            let (name, color) = PRESETS[idx];
            let row_style = if idx == self.selected {
                selected_style
            } else {
                normal
            };
            ctx.fill(Rect::new(body.a.x, y, body.b.x, y + 1), ' ', row_style);
            let swatch = match color_to_display_rgb(color) {
                Some((r, g, b)) => Style::new(Color::Rgb(r, g, b), Color::Rgb(r, g, b)),
                None => row_style,
            };
            ctx.fill(Rect::new(body.a.x, y, body.a.x + 2, y + 1), ' ', swatch);
            ctx.put_str(body.a.x + 3, y, name, row_style);
        }
    }

    fn handle_event(&mut self, ev: &mut Event, body: Rect, m: &mut ColorModel, _ctx: &mut Context) {
        let rows = (body.b.y - body.a.y) as usize;
        match *ev {
            Event::KeyDown(ke) => {
                let ke = ctrl_to_arrow(ke);
                match ke.key {
                    Key::Up => {
                        if self.selected > 0 {
                            self.selected -= 1;
                        }
                    }
                    Key::Down => {
                        if self.selected + 1 < PRESETS.len() {
                            self.selected += 1;
                        }
                    }
                    _ => return,
                }
                self.scroll_into_view(rows);
                m.set_color(PRESETS[self.selected].1);
                ev.clear();
            }
            Event::MouseDown(me) => {
                let y = me.position.y - body.a.y;
                if y >= 0 && me.position.x >= body.a.x && me.position.x < body.b.x {
                    let idx = self.top + y as usize;
                    if idx < PRESETS.len() {
                        self.selected = idx;
                        m.set_color(PRESETS[idx].1);
                        ev.clear();
                    }
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
        b: Point { x: 37, y: 18 },
    };

    #[test]
    fn first_entry_is_default() {
        assert_eq!(PRESETS[0].1, Color::Default);
        assert_eq!(PRESETS[1].1, Color::Bios(0)); // Black
    }

    #[test]
    fn preset_table_has_default_16_bios_and_12_rgb() {
        assert_eq!(PRESETS.len(), 1 + 16 + 12);
    }

    #[test]
    fn down_arrow_advances_selection_and_sets_color() {
        let mut s = PresetsSurface::new(&ColorModel::new(Color::Default));
        let mut m = ColorModel::new(Color::Default);
        let mut ev = key(Key::Down);
        with_ctx(|ctx| {
            <PresetsSurface as crate::dialog::colorpick::Surface>::handle_event(
                &mut s, &mut ev, BODY, &mut m, ctx,
            )
        });
        assert_eq!(m.color, Color::Bios(0)); // moved Default -> Black
        assert!(ev.is_nothing());
    }

    #[test]
    fn up_arrow_at_top_does_not_wrap() {
        let mut s = PresetsSurface::new(&ColorModel::new(Color::Default));
        let mut m = ColorModel::new(Color::Default);
        let mut ev = key(Key::Up);
        with_ctx(|ctx| {
            <PresetsSurface as crate::dialog::colorpick::Surface>::handle_event(
                &mut s, &mut ev, BODY, &mut m, ctx,
            )
        });
        assert_eq!(m.color, Color::Default); // clamped at top
    }

    #[test]
    fn new_seeds_selection_from_model_color() {
        let s = PresetsSurface::new(&ColorModel::new(Color::Bios(4)));
        assert_eq!(PRESETS[s.selected].1, Color::Bios(4));
    }

    fn render(s: &PresetsSurface, m: &ColorModel) -> String {
        use crate::backend::{HeadlessBackend, Renderer};
        use crate::screen::Buffer;
        use crate::theme::Theme;
        let theme = Theme::classic_blue();
        let (backend, screen) = HeadlessBackend::new(40, 18);
        let mut r = Renderer::new(Box::new(backend));
        r.render(|buf: &mut Buffer| {
            let bounds = Rect::new(0, 0, 40, 18);
            let mut dc = crate::view::DrawCtx::new(buf, &theme, bounds, bounds.a);
            <PresetsSurface as crate::dialog::colorpick::Surface>::draw(s, &mut dc, BODY, m);
        });
        screen.snapshot()
    }

    #[test]
    fn snapshot_presets_at_red() {
        let m = ColorModel::new(Color::Bios(4));
        let s = PresetsSurface::new(&m);
        insta::assert_snapshot!(render(&s, &m));
    }
}
