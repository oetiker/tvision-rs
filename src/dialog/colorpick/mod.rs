//! Truecolor color-picker — an rstv-original extension (NOT a faithful port).
//!
//! One [`ColorPicker`] view owns a shared [`model::ColorModel`]; four surfaces
//! draw + handle events against it. Produces any [`Color`](crate::color::Color)
//! variant.

pub(crate) mod drag;
pub mod model;
pub(crate) mod plane;
pub(crate) mod presets;
pub(crate) mod rgb;
pub(crate) mod xterm256;

use crate::color::{Color, Style};
use crate::event::{Event, Key, hot_key};
use crate::view::{Context, DrawCtx, Options, Point, Rect, View, ViewState};
use model::{ColorModel, color_to_display_rgb};

// -- shared layout (picker-local) ---------------------------------------------
/// Picker-local tab-bar row.
pub(crate) const TAB_BAR_Y: i32 = 0;
/// Picker-local x where the info column starts (right edge of the surface body).
pub(crate) const INFO_COL_X: i32 = 38;
/// Picker-local body top (first row below the tab bar).
pub(crate) const BODY_TOP: i32 = 1;

/// A picker surface — draws + handles events against the shared [`ColorModel`].
pub(crate) trait Surface {
    fn draw(&self, ctx: &mut DrawCtx, body: Rect, m: &ColorModel);
    fn handle_event(&mut self, ev: &mut Event, body: Rect, m: &mut ColorModel, ctx: &mut Context);
    fn drag_region_at(&self, _p: Point, _body: Rect) -> Option<drag::ColorDragRegion> {
        None
    }
    fn apply_drag(
        &mut self,
        _region: drag::ColorDragRegion,
        _p: Point,
        _body: Rect,
        _m: &mut ColorModel,
    ) {
    }
}

// -- Tab enum -----------------------------------------------------------------

/// The active surface tab.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Tab {
    Presets,
    Rgb,
    Plane,
    Xterm256,
}

impl Tab {
    const ORDER: [Tab; 4] = [Tab::Presets, Tab::Rgb, Tab::Plane, Tab::Xterm256];

    /// Label for the tab bar, with `~X~` marking the hotkey letter.
    fn label(self) -> &'static str {
        match self {
            Tab::Presets => "~P~resets",
            Tab::Rgb => "~R~GB",
            Tab::Plane => "Plane ~W~",
            Tab::Xterm256 => "~6~",
        }
    }

    fn idx(self) -> usize {
        Self::ORDER.iter().position(|&t| t == self).unwrap()
    }

    fn cycle(self, forward: bool) -> Tab {
        let i = self.idx();
        let n = Self::ORDER.len();
        Self::ORDER[if forward {
            (i + 1) % n
        } else {
            (i + n - 1) % n
        }]
    }
}

// -- ColorPicker struct -------------------------------------------------------

/// The reusable, embeddable truecolor color-picker view (Approach A).
/// Owns the shared [`ColorModel`] + the four surfaces. Does NOT own OK/Cancel (dialog chrome).
pub struct ColorPicker {
    state: ViewState,
    model: ColorModel,
    pub(crate) active: Tab,
    presets: presets::PresetsSurface,
    rgb: rgb::RgbSurface,
    plane: plane::PlaneSurface,
    grid: xterm256::Xterm256Surface,
    /// Picker-local origin (= ctx.origin()), cached each draw for the drag handler.
    #[allow(dead_code)]
    body_origin: Point,
    /// The drag region being scrubbed, set when the drag capture is pushed.
    #[allow(dead_code)]
    pub(crate) active_drag: Option<drag::ColorDragRegion>,
    /// The initial (old) color — shown in the info column as the "before" swatch.
    old: Color,
}

impl ColorPicker {
    #[allow(dead_code)]
    pub fn new(bounds: Rect, initial: Color) -> Self {
        let mut state = ViewState::new(bounds);
        state.options = Options {
            selectable: true,
            first_click: true,
            ..Default::default()
        };
        let model = ColorModel::new(initial);
        ColorPicker {
            presets: presets::PresetsSurface::new(&model),
            rgb: rgb::RgbSurface::new(),
            plane: plane::PlaneSurface::new(),
            grid: xterm256::Xterm256Surface::new(&model),
            model,
            active: Tab::Presets,
            state,
            body_origin: Point::new(0, 0),
            active_drag: None,
            old: initial,
        }
    }

    /// The current selection — the contract `color_dialog` reads.
    #[allow(dead_code)]
    pub fn color(&self) -> Color {
        self.model.color
    }

    /// Picker-local surface body rect (left of the info column, below the tab bar).
    fn body_rect(&self) -> Rect {
        let sz = self.state.size;
        Rect::new(0, BODY_TOP, INFO_COL_X, sz.y)
    }

    fn active_surface(&self) -> &dyn Surface {
        match self.active {
            Tab::Presets => &self.presets,
            Tab::Rgb => &self.rgb,
            Tab::Plane => &self.plane,
            Tab::Xterm256 => &self.grid,
        }
    }

    /// Apply a drag broker callback. Called by the pump's deferred-apply arm.
    #[allow(dead_code)]
    pub(crate) fn apply_drag(&mut self, pos: Point) {
        let body = self.body_rect();
        if let Some(region) = self.active_drag {
            // Inline match to avoid borrowing all of self through a method call
            let model = &mut self.model;
            match self.active {
                Tab::Presets => self.presets.apply_drag(region, pos, body, model),
                Tab::Rgb => self.rgb.apply_drag(region, pos, body, model),
                Tab::Plane => self.plane.apply_drag(region, pos, body, model),
                Tab::Xterm256 => self.grid.apply_drag(region, pos, body, model),
            }
        }
    }

    /// Draw the info column (right of the body, picker-local x 38..56, rows 1..sz.y).
    /// Shows: old swatch + new swatch + variant readout.
    fn draw_info_column(&self, ctx: &mut DrawCtx) {
        let sz = self.state.size;
        let normal = ctx.style(crate::theme::Role::ScrollerNormal);
        let info_rect = Rect::new(INFO_COL_X, BODY_TOP, sz.x, sz.y);
        ctx.fill(info_rect, ' ', normal);

        // Label
        ctx.put_str(INFO_COL_X + 1, BODY_TOP, "Old:", normal);
        // Old swatch (2 cells)
        let old_swatch = match color_to_display_rgb(self.old) {
            Some((r, g, b)) => Style::new(Color::Rgb(r, g, b), Color::Rgb(r, g, b)),
            None => normal,
        };
        ctx.fill(
            Rect::new(INFO_COL_X + 1, BODY_TOP + 1, INFO_COL_X + 5, BODY_TOP + 2),
            ' ',
            old_swatch,
        );

        ctx.put_str(INFO_COL_X + 1, BODY_TOP + 2, "New:", normal);
        // New swatch (2 cells)
        let new_swatch = match color_to_display_rgb(self.model.color) {
            Some((r, g, b)) => Style::new(Color::Rgb(r, g, b), Color::Rgb(r, g, b)),
            None => normal,
        };
        ctx.fill(
            Rect::new(INFO_COL_X + 1, BODY_TOP + 3, INFO_COL_X + 5, BODY_TOP + 4),
            ' ',
            new_swatch,
        );

        // Variant readout
        let variant_str = match self.model.color {
            Color::Rgb(r, g, b) => format!("Rgb({},{},{})", r, g, b),
            Color::Bios(n) => {
                let bios_names = [
                    "Black", "Blue", "Green", "Cyan", "Red", "Magenta", "Brown", "LGray", "DGray",
                    "LBlue", "LGreen", "LCyan", "LRed", "LMag", "Yellow", "White",
                ];
                let name = bios_names.get(n as usize).copied().unwrap_or("?");
                format!("Bios({}) {}", n, name)
            }
            Color::Indexed(n) => format!("Idx({})", n),
            Color::Default => "Default".to_string(),
        };
        ctx.put_str(INFO_COL_X + 1, BODY_TOP + 5, &variant_str, normal);
    }
}

impl View for ColorPicker {
    fn state(&self) -> &ViewState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.state
    }

    fn draw(&mut self, ctx: &mut DrawCtx) {
        self.body_origin = ctx.origin();
        let sz = self.state.size;
        let bar_bg = ctx.style(crate::theme::Role::FramePassive);
        let tab_normal = ctx.style(crate::theme::Role::ButtonNormal);
        let tab_active = ctx.style(crate::theme::Role::ButtonSelected);

        // Tab bar background
        ctx.fill(Rect::new(0, TAB_BAR_Y, sz.x, TAB_BAR_Y + 1), ' ', bar_bg);

        // Draw tab labels using put_cstr (handles ~X~ hotkey toggle)
        let mut x = 1;
        for t in Tab::ORDER {
            let style = if t == self.active {
                tab_active
            } else {
                tab_normal
            };
            // hotkey rendered in same style as rest for now
            let w = ctx.put_cstr(x, TAB_BAR_Y, t.label(), style, style);
            x += w + 1;
        }

        // Body
        let body = self.body_rect();
        self.active_surface().draw(ctx, body, &self.model);

        // Info column
        self.draw_info_column(ctx);
    }

    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        // 1. Tab switching: Ctrl+Left/Right
        if let Event::KeyDown(ke) = *ev {
            if ke.modifiers.ctrl && matches!(ke.key, Key::Left | Key::Right) {
                self.active = self.active.cycle(matches!(ke.key, Key::Right));
                ev.clear();
                return;
            }
            // Alt+hotkey jumps to a tab
            if ke.modifiers.alt
                && let Key::Char(c) = ke.key
            {
                let up = c.to_ascii_uppercase();
                for t in Tab::ORDER {
                    if hot_key(t.label()) == Some(up) {
                        self.active = t;
                        ev.clear();
                        return;
                    }
                }
            }
            // Leave plain Tab/Shift+Tab for the dialog
            if ke.key == Key::Tab {
                return;
            }
        }
        // Tab-label click in the tab bar
        if let Event::MouseDown(me) = *ev
            && me.position.y == TAB_BAR_Y
        {
            let mut x = 1i32;
            for t in Tab::ORDER {
                // Width of this tab label without the ~ chars
                let w = t.label().chars().filter(|&c| c != '~').count() as i32;
                if me.position.x >= x && me.position.x < x + w {
                    self.active = t;
                    ev.clear();
                    return;
                }
                x += w + 1;
            }
        }
        // Drag: if the active surface reports a draggable region at a MouseDown,
        // record it in `active_drag`, apply the down-click immediately (so a plain
        // click still works), and push a ColorDragCapture so subsequent MouseMove
        // events keep scrubbing. Picker-local coords throughout — no pre-subtraction
        // of BODY_TOP (surfaces subtract body.a exactly once).
        let body = self.body_rect();
        if let Event::MouseDown(me) = *ev {
            // Borrow surfaces read-only first to check for a drag region.
            let region = match self.active {
                Tab::Presets => self.presets.drag_region_at(me.position, body),
                Tab::Rgb => self.rgb.drag_region_at(me.position, body),
                Tab::Plane => self.plane.drag_region_at(me.position, body),
                Tab::Xterm256 => self.grid.drag_region_at(me.position, body),
            };
            if let Some(region) = region
                && let Some(id) = self.state.id()
            {
                self.active_drag = Some(region);
                let origin = self.body_origin;
                // Apply the down-click immediately (single click works).
                match self.active {
                    Tab::Presets => {
                        self.presets
                            .apply_drag(region, me.position, body, &mut self.model)
                    }
                    Tab::Rgb => self
                        .rgb
                        .apply_drag(region, me.position, body, &mut self.model),
                    Tab::Plane => self
                        .plane
                        .apply_drag(region, me.position, body, &mut self.model),
                    Tab::Xterm256 => {
                        self.grid
                            .apply_drag(region, me.position, body, &mut self.model)
                    }
                }
                // Push a capture so subsequent moves keep scrubbing.
                ctx.push_capture(Box::new(drag::ColorDragCapture::new(id, origin)));
                ev.clear();
                return;
            }
        }
        // Delegate to the active surface — inline match to split borrows correctly
        let model = &mut self.model;
        match self.active {
            Tab::Presets => self.presets.handle_event(ev, body, model, ctx),
            Tab::Rgb => self.rgb.handle_event(ev, body, model, ctx),
            Tab::Plane => self.plane.handle_event(ev, body, model, ctx),
            Tab::Xterm256 => self.grid.handle_event(ev, body, model, ctx),
        }
    }

    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }
}

// -- Tests --------------------------------------------------------------------

#[cfg(test)]
mod view_tests {
    use super::*;
    use crate::color::Color;
    use crate::event::{Event, Key, KeyEvent, KeyModifiers};
    use crate::timer::TimerQueue;
    use crate::view::{Context, Deferred, Rect, View};
    use std::collections::VecDeque;

    fn with_ctx<R>(f: impl FnOnce(&mut Context) -> R) -> R {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
        f(&mut ctx)
    }

    fn ctrl(k: Key) -> Event {
        Event::KeyDown(KeyEvent::new(
            k,
            KeyModifiers {
                ctrl: true,
                ..Default::default()
            },
        ))
    }

    fn plain(k: Key) -> Event {
        Event::KeyDown(KeyEvent::new(k, KeyModifiers::default()))
    }

    #[test]
    fn color_returns_seed() {
        let p = ColorPicker::new(Rect::new(0, 0, 56, 18), Color::Rgb(30, 144, 255));
        assert_eq!(p.color(), Color::Rgb(30, 144, 255));
    }

    #[test]
    fn ctrl_right_cycles_tab_forward() {
        let mut p = ColorPicker::new(Rect::new(0, 0, 56, 18), Color::Default);
        assert_eq!(p.active, Tab::Presets);
        let mut ev = ctrl(Key::Right);
        with_ctx(|ctx| p.handle_event(&mut ev, ctx));
        assert_eq!(p.active, Tab::Rgb);
        assert!(ev.is_nothing());
    }

    #[test]
    fn ctrl_left_cycles_tab_backward_with_wrap() {
        let mut p = ColorPicker::new(Rect::new(0, 0, 56, 18), Color::Default);
        let mut ev = ctrl(Key::Left);
        with_ctx(|ctx| p.handle_event(&mut ev, ctx));
        assert_eq!(p.active, Tab::Xterm256); // wrapped
    }

    #[test]
    fn plain_tab_is_left_unhandled() {
        let mut p = ColorPicker::new(Rect::new(0, 0, 56, 18), Color::Default);
        let mut ev = plain(Key::Tab);
        with_ctx(|ctx| p.handle_event(&mut ev, ctx));
        assert!(!ev.is_nothing(), "plain Tab must pass to dialog");
    }

    #[test]
    fn switching_tab_does_not_change_color() {
        let mut p = ColorPicker::new(Rect::new(0, 0, 56, 18), Color::Rgb(10, 20, 30));
        let mut ev = ctrl(Key::Right);
        with_ctx(|ctx| p.handle_event(&mut ev, ctx));
        assert_eq!(p.color(), Color::Rgb(10, 20, 30));
    }
}

#[cfg(test)]
mod snap_tests {
    use super::*;
    use crate::backend::{HeadlessBackend, Renderer};
    use crate::color::Color;
    use crate::screen::Buffer;
    use crate::theme::Theme;
    use crate::view::{DrawCtx, Rect, View};

    fn render_picker(initial: Color, active: Tab) -> String {
        let theme = Theme::classic_blue();
        // Use a compact 40×12 backend to keep snapshot legend under control
        let (backend, screen) = HeadlessBackend::new(40, 12);
        let mut r = Renderer::new(Box::new(backend));
        let mut p = ColorPicker::new(Rect::new(0, 0, 40, 12), initial);
        p.active = active;
        r.render(|buf: &mut Buffer| {
            let bounds = Rect::new(0, 0, 40, 12);
            let mut dc = DrawCtx::new(buf, &theme, bounds, bounds.a);
            p.draw(&mut dc);
        });
        screen.snapshot()
    }

    #[test]
    fn snapshot_picker_presets() {
        insta::assert_snapshot!(render_picker(Color::Bios(4), Tab::Presets));
    }

    // snapshot_picker_rgb skipped: the RGB gradient bars produce one unique style
    // per pixel (bar_w=32 × 3 bars = 96 styles), exhausting the 62-char legend.
    // The rgb-standalone snapshot in rgb.rs covers the surface itself on a
    // narrower canvas that fits within the legend limit.

    // xterm256 surface snapshot skipped: the 16×16 palette grid uses more than
    // 63 distinct styles in 40×12, exhausting the snapshot legend. The
    // xterm256-standalone snapshot in xterm256.rs covers the surface itself.
}
