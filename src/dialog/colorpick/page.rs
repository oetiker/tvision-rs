//! A page View wrapping one picker [`Surface`](super::Surface): it owns the
//! shared [`SharedModel`](super::SharedModel) and bridges the surface's
//! `body`-relative draw/handle to the View trait. Draggable surfaces self-drive
//! via the standard mouse-track capture (the `ScrollBar` thumb-drag pattern).

use super::drag::ColorDragRegion;
use super::{SharedModel, Surface};
use crate::capture::TrackMask;
use crate::event::Event;
use crate::view::{Context, DrawCtx, Point, Rect, View, ViewState};

pub(crate) struct SurfacePage<S: Surface> {
    state: ViewState,
    surface: S,
    model: SharedModel,
    /// Absolute screen position of page-local (0,0) — cached each `draw` for the
    /// track capture (so routed MouseMove events arrive page-local).
    abs_origin: Point,
    tracking: bool,
    region: Option<ColorDragRegion>,
}

impl<S: Surface> SurfacePage<S> {
    pub(crate) fn new(bounds: Rect, surface: S, model: SharedModel) -> Self {
        SurfacePage {
            state: ViewState::new(bounds),
            surface,
            model,
            abs_origin: bounds.a,
            tracking: false,
            region: None,
        }
    }

    /// Page-local body rect (the whole page extent; surfaces subtract `body.a`).
    fn body(&self) -> Rect {
        let s = self.state.size;
        Rect::new(0, 0, s.x, s.y)
    }
}

impl<S: Surface + 'static> View for SurfacePage<S> {
    fn state(&self) -> &ViewState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.state
    }
    fn as_any_mut(&mut self) -> Option<&mut dyn core::any::Any> {
        Some(self)
    }

    fn draw(&mut self, ctx: &mut DrawCtx) {
        self.abs_origin = ctx.origin();
        let body = self.body();
        let m = self.model.borrow();
        self.surface.draw(ctx, body, &m);
    }

    fn handle_event(&mut self, ev: &mut Event, ctx: &mut Context) {
        let body = self.body();
        match *ev {
            Event::MouseDown(me) => {
                if let Some(region) = self.surface.drag_region_at(me.position, body) {
                    self.region = Some(region);
                    {
                        let mut m = self.model.borrow_mut();
                        self.surface.apply_drag(region, me.position, body, &mut m);
                    }
                    if let Some(id) = self.state.id() {
                        self.tracking = true;
                        ctx.start_mouse_track(
                            id,
                            self.abs_origin,
                            TrackMask {
                                mouse_move: true,
                                ..Default::default()
                            },
                        );
                    }
                    ev.clear();
                    return;
                }
                let mut m = self.model.borrow_mut();
                self.surface.handle_event(ev, body, &mut m, ctx);
            }
            Event::MouseMove(me) if self.tracking => {
                if let Some(region) = self.region {
                    let mut m = self.model.borrow_mut();
                    self.surface.apply_drag(region, me.position, body, &mut m);
                }
                ev.clear();
            }
            Event::MouseUp(_) if self.tracking => {
                self.tracking = false;
                self.region = None;
                ev.clear();
            }
            _ => {
                let mut m = self.model.borrow_mut();
                self.surface.handle_event(ev, body, &mut m, ctx);
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;
    use crate::dialog::colorpick::model::ColorModel;
    use crate::dialog::colorpick::presets::PresetsSurface;
    use crate::dialog::colorpick::rgb::RgbSurface;
    use crate::event::{KeyModifiers, MouseButtons, MouseEvent, MouseEventFlags, MouseWheel};
    use crate::timer::TimerQueue;
    use crate::view::{Deferred, ViewId};
    use std::cell::RefCell;
    use std::collections::VecDeque;
    use std::rc::Rc;

    fn with_ctx<R>(f: impl FnOnce(&mut Context) -> R) -> R {
        let mut out = VecDeque::new();
        let mut timers = TimerQueue::new();
        let mut deferred: Vec<Deferred> = Vec::new();
        let mut ctx = Context::new(&mut out, &mut timers, 0, &mut deferred);
        f(&mut ctx)
    }

    fn mouse(kind: fn(MouseEvent) -> Event, x: i32, y: i32) -> Event {
        kind(MouseEvent {
            position: Point::new(x, y),
            buttons: MouseButtons {
                left: true,
                ..Default::default()
            },
            flags: MouseEventFlags::default(),
            wheel: MouseWheel::None,
            modifiers: KeyModifiers::default(),
        })
    }

    fn shared(c: Color) -> SharedModel {
        Rc::new(RefCell::new(ColorModel::new(c)))
    }

    #[test]
    fn rgb_page_drag_applies_on_down_then_scrubs_on_move() {
        // 22-wide body: RgbSurface lays bars at body.a.x+6=6, bar_w=16. The R bar
        // is at page-local y = R_ROW (1). A MouseDown there starts a scrub.
        let model = shared(Color::Rgb(30, 144, 255));
        let mut page = SurfacePage::new(Rect::new(0, 0, 22, 18), RgbSurface::new(), model.clone());
        page.state.id = Some(ViewId::next());

        // Far-left of the R bar → R channel ≈ 0.
        let mut down = mouse(Event::MouseDown, 6, 1);
        with_ctx(|ctx| page.handle_event(&mut down, ctx));
        let after_down = model.borrow().color;
        assert_ne!(
            after_down,
            Color::Rgb(30, 144, 255),
            "drag applied on the down-click"
        );
        if let Color::Rgb(r, _, _) = after_down {
            assert_eq!(r, 0, "left edge of the R bar sets R=0");
        } else {
            panic!("expected Rgb, got {after_down:?}");
        }

        // Move toward the right end of the bar → R increases.
        let mut mv = mouse(Event::MouseMove, 21, 1);
        with_ctx(|ctx| page.handle_event(&mut mv, ctx));
        if let Color::Rgb(r, _, _) = model.borrow().color {
            assert!(r > 0, "move scrubs the R channel up from 0, got {r}");
        } else {
            panic!("expected Rgb after move");
        }
    }

    #[test]
    fn presets_page_click_delegates_to_surface() {
        // PresetsSurface lists Default at row 0; clicking row 1 selects "Black"
        // (Bios(0)). Delegation through the non-drag handle_event path.
        let model = shared(Color::Rgb(30, 144, 255));
        let mut page = SurfacePage::new(
            Rect::new(0, 0, 22, 18),
            PresetsSurface::new(&model.borrow()),
            model.clone(),
        );
        page.state.id = Some(ViewId::next());

        let mut down = mouse(Event::MouseDown, 3, 1);
        with_ctx(|ctx| page.handle_event(&mut down, ctx));
        assert_eq!(
            model.borrow().color,
            Color::Bios(0),
            "clicking row 1 selects the second preset (Black)"
        );
    }
}
