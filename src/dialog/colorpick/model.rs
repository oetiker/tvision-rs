//! [`ColorModel`] — the picker's single source of truth, plus the conversions.

// Some model helpers are not exercised by every surface.
#![allow(dead_code)]

use crate::backend::xterm256_to_rgb;
use crate::color::Color;

/// Canonical BIOS→RGB palette (re-exported from `crate::color::Color`).
pub const BIOS_RGB: [(u8, u8, u8); 16] = crate::color::Color::BIOS_RGB;

/// Working hue/sat/val. `h` is degrees `0.0..360.0`; `s`,`v` are `0.0..1.0`.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct Hsv {
    pub h: f32,
    pub s: f32,
    pub v: f32,
}

/// HSV → RGB (standard sextant formula). Each channel rounds half-up:
/// `(c * 255.0 + 0.5).clamp(0.0, 255.0) as u8`.
pub fn hsv_to_rgb(hsv: Hsv) -> (u8, u8, u8) {
    let h = hsv.h.rem_euclid(360.0);
    let s = hsv.s.clamp(0.0, 1.0);
    let v = hsv.v.clamp(0.0, 1.0);
    let c = v * s;
    let h6 = h / 60.0;
    let x = c * (1.0 - (h6.rem_euclid(2.0) - 1.0).abs());
    let (r1, g1, b1) = match h6 as u8 {
        0 => (c, x, 0.0),
        1 => (x, c, 0.0),
        2 => (0.0, c, x),
        3 => (0.0, x, c),
        4 => (x, 0.0, c),
        _ => (c, 0.0, x),
    };
    let m = v - c;
    let to_u8 = |f: f32| ((f + m) * 255.0 + 0.5).clamp(0.0, 255.0) as u8;
    (to_u8(r1), to_u8(g1), to_u8(b1))
}

/// RGB → HSV (standard). Hue 0 when chroma is 0.
pub fn rgb_to_hsv(r: u8, g: u8, b: u8) -> Hsv {
    let rf = r as f32 / 255.0;
    let gf = g as f32 / 255.0;
    let bf = b as f32 / 255.0;
    let max = rf.max(gf).max(bf);
    let min = rf.min(gf).min(bf);
    let c = max - min;
    let h = if c == 0.0 {
        0.0
    } else if max == rf {
        60.0 * (((gf - bf) / c).rem_euclid(6.0))
    } else if max == gf {
        60.0 * ((bf - rf) / c + 2.0)
    } else {
        60.0 * ((rf - gf) / c + 4.0)
    };
    let s = if max == 0.0 { 0.0 } else { c / max };
    Hsv {
        h: h.rem_euclid(360.0),
        s,
        v: max,
    }
}

/// The display RGB for a [`Color`], or `None` for [`Color::Default`].
pub fn color_to_display_rgb(c: Color) -> Option<(u8, u8, u8)> {
    match c {
        Color::Default => None,
        Color::Bios(n) => Some(BIOS_RGB[(n & 0x0F) as usize]),
        Color::Indexed(n) => Some(xterm256_to_rgb(n)),
        Color::Rgb(r, g, b) => Some((r, g, b)),
    }
}

/// The picker's single source of truth (Approach A). `color` is the committed
/// selection (its variant *is* the mode); `hsv` is retained working HSV.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct ColorModel {
    pub color: Color,
    pub hsv: Hsv,
}

impl ColorModel {
    pub fn new(color: Color) -> Self {
        let (r, g, b) = color_to_display_rgb(color).unwrap_or((0, 0, 0));
        ColorModel {
            color,
            hsv: rgb_to_hsv(r, g, b),
        }
    }

    pub fn set_color(&mut self, c: Color) {
        self.color = c;
        if let Some((r, g, b)) = color_to_display_rgb(c) {
            self.hsv = rgb_to_hsv(r, g, b);
        }
    }

    pub fn set_rgb(&mut self, r: u8, g: u8, b: u8) {
        self.color = Color::Rgb(r, g, b);
        self.hsv = rgb_to_hsv(r, g, b);
    }

    pub fn set_indexed(&mut self, idx: u8) {
        self.color = Color::Indexed(idx);
        let (r, g, b) = xterm256_to_rgb(idx);
        self.hsv = rgb_to_hsv(r, g, b);
    }

    pub fn set_hsv(&mut self, hsv: Hsv) {
        self.hsv = hsv;
        let (r, g, b) = hsv_to_rgb(hsv);
        self.color = Color::Rgb(r, g, b);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::color::Color;

    fn approx(a: f32, b: f32, eps: f32) -> bool {
        (a - b).abs() <= eps
    }

    #[test]
    fn hsv_to_rgb_primaries() {
        assert_eq!(
            hsv_to_rgb(Hsv {
                h: 0.0,
                s: 1.0,
                v: 1.0
            }),
            (255, 0, 0)
        ); // red
        assert_eq!(
            hsv_to_rgb(Hsv {
                h: 120.0,
                s: 1.0,
                v: 1.0
            }),
            (0, 255, 0)
        ); // green
        assert_eq!(
            hsv_to_rgb(Hsv {
                h: 240.0,
                s: 1.0,
                v: 1.0
            }),
            (0, 0, 255)
        ); // blue
        assert_eq!(
            hsv_to_rgb(Hsv {
                h: 0.0,
                s: 0.0,
                v: 1.0
            }),
            (255, 255, 255)
        ); // white
        assert_eq!(
            hsv_to_rgb(Hsv {
                h: 0.0,
                s: 0.0,
                v: 0.0
            }),
            (0, 0, 0)
        ); // black
    }

    #[test]
    fn rgb_to_hsv_primaries() {
        let h = rgb_to_hsv(255, 0, 0);
        assert!(approx(h.h, 0.0, 0.5) && approx(h.s, 1.0, 0.01) && approx(h.v, 1.0, 0.01));
        let h = rgb_to_hsv(0, 0, 255);
        assert!(approx(h.h, 240.0, 0.5));
    }

    #[test]
    fn rgb_hsv_round_trip_is_stable_for_saturated_colors() {
        for (r, g, b) in [(30u8, 144, 255), (255, 165, 0), (128, 0, 128)] {
            let (r2, g2, b2) = hsv_to_rgb(rgb_to_hsv(r, g, b));
            assert!((r as i16 - r2 as i16).abs() <= 1, "r {r}->{r2}");
            assert!((g as i16 - g2 as i16).abs() <= 1, "g {g}->{g2}");
            assert!((b as i16 - b2 as i16).abs() <= 1, "b {b}->{b2}");
        }
    }

    #[test]
    fn model_set_rgb_sets_variant_and_refreshes_hsv() {
        let mut m = ColorModel::new(Color::Default);
        m.set_rgb(255, 0, 0);
        assert_eq!(m.color, Color::Rgb(255, 0, 0));
        assert!(approx(m.hsv.h, 0.0, 0.5) && approx(m.hsv.s, 1.0, 0.01));
    }

    #[test]
    fn model_set_indexed_sets_variant() {
        let mut m = ColorModel::new(Color::Default);
        m.set_indexed(33);
        assert_eq!(m.color, Color::Indexed(33));
    }

    #[test]
    fn model_set_color_preset_keeps_variant() {
        let mut m = ColorModel::new(Color::Default);
        m.set_color(Color::Bios(4));
        assert_eq!(m.color, Color::Bios(4));
    }

    #[test]
    fn hsv_retention_keeps_hue_through_black_and_back() {
        let mut m = ColorModel::new(Color::Rgb(255, 165, 0));
        let hue0 = m.hsv.h;
        m.set_hsv(Hsv { v: 0.0, ..m.hsv });
        assert_eq!(m.color, Color::Rgb(0, 0, 0));
        assert!(approx(m.hsv.h, hue0, 0.5), "hue must be retained at v=0");
        m.set_hsv(Hsv { v: 1.0, ..m.hsv });
        assert!(
            approx(m.hsv.h, hue0, 0.5),
            "hue must survive the round-trip"
        );
    }

    #[test]
    fn hsv_retention_keeps_hue_through_gray() {
        let mut m = ColorModel::new(Color::Rgb(0, 0, 255));
        let hue0 = m.hsv.h;
        m.set_hsv(Hsv { s: 0.0, ..m.hsv });
        assert!(approx(m.hsv.h, hue0, 0.5), "hue retained at s=0");
    }

    #[test]
    fn bios_rgb_table_has_16_canonical_entries() {
        assert_eq!(BIOS_RGB[0], (0, 0, 0)); // Black
        assert_eq!(BIOS_RGB[1], (0, 0, 170)); // Blue
        assert_eq!(BIOS_RGB[7], (170, 170, 170)); // Light Gray
        assert_eq!(BIOS_RGB[8], (85, 85, 85)); // Dark Gray
        assert_eq!(BIOS_RGB[15], (255, 255, 255)); // White
    }

    #[test]
    fn display_rgb_maps_each_variant() {
        assert_eq!(
            color_to_display_rgb(Color::Rgb(30, 144, 255)),
            Some((30, 144, 255))
        );
        assert_eq!(color_to_display_rgb(Color::Bios(4)), Some((170, 0, 0))); // Red
        // Indexed uses the quantize.rs xterm-256 table.
        assert_eq!(color_to_display_rgb(Color::Indexed(16)), Some((0, 0, 0)));
        assert_eq!(
            color_to_display_rgb(Color::Indexed(231)),
            Some((255, 255, 255))
        );
        // Default has no concrete RGB (swatch shows a "default" marker).
        assert_eq!(color_to_display_rgb(Color::Default), None);
    }
}
