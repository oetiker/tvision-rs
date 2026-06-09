//! Truecolor color-picker — an rstv-original extension (NOT a faithful port).
//!
//! See `docs/superpowers/specs/2026-06-09-color-picker-design.md`. One
//! [`ColorPicker`] view owns a shared [`model::ColorModel`]; four surfaces draw +
//! handle events against it. Produces any [`Color`](crate::color::Color) variant.

pub mod model;
