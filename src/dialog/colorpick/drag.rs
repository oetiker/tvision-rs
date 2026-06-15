//! Draggable-region identity for the color picker's surfaces.
//!
//! A [`Surface`](super::Surface) reports which region a pointer is over via
//! [`Surface::drag_region_at`](super::Surface::drag_region_at) and consumes a
//! scrub via [`Surface::apply_drag`](super::Surface::apply_drag). The drag itself
//! is driven by the page View ([`SurfacePage`](super::page::SurfacePage)) through
//! the standard mouse-track capture (the `ScrollBar`/`TabBar` thumb-drag pattern)
//! — there is no bespoke capture handler here.

/// Which draggable region of the active surface a drag is scrubbing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorDragRegion {
    /// The HSV plane's Saturation×Value box.
    SvBox,
    /// The HSV plane's vertical hue strip.
    HueStrip,
    /// An RGB gauge bar — `0`=R, `1`=G, `2`=B.
    RgbBar(u8),
}
