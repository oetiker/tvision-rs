//! Truecolor color-picker — an rstv-original extension (NOT a faithful port).
//!
//! See `docs/superpowers/specs/2026-06-09-color-picker-design.md`. One
//! [`ColorPicker`] view owns a shared [`model::ColorModel`]; four surfaces draw +
//! handle events against it. Produces any [`Color`](crate::color::Color) variant.

// Color-picker scaffolding: consts, trait, and submodule items are consumed
// progressively across tasks; allow until the ColorPicker view wires them up.
#![allow(dead_code, unused_imports)]

pub(crate) mod drag;
pub mod model;
pub(crate) mod plane;
pub(crate) mod presets;
pub(crate) mod rgb;

use crate::color::Color;
use crate::event::Event;
use crate::view::{Context, DrawCtx, Point, Rect};
use model::ColorModel;

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
