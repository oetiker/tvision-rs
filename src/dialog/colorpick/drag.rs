//! Mouse-drag capture for the color picker.

// Enum variants are consumed progressively across tasks; allow until wired up.
#![allow(dead_code)]

/// Which draggable region of the active surface a drag is scrubbing.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ColorDragRegion {
    SvBox,
    HueStrip,
    RgbBar(u8),
}
