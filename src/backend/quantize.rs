//! RGB colour-quantization ladder — deviation **D6**.
//!
//! Faithful port of `RGBtoXTerm16`, `RGBtoXTerm256`, `BIOStoXTerm16`, and
//! related helpers from `source/platform/colors.cpp` and
//! `include/tvision/colors.h` (magiblot/tvision).
//!
//! These are pure math functions operating on raw `u8` channel values and
//! palette indices. They are deliberately decoupled from the [`Color`] enum —
//! row 19 (`Backend`) assembles the capability-aware policy that decides which
//! rung of the ladder to use.
//!
//! [`Color`]: crate::color::Color

// ---------------------------------------------------------------------------
// HCL helper (RGBtoHCL, colors.cpp)
// ---------------------------------------------------------------------------

/// Hue-Chroma-Lightness record used internally by [`rgb_to_xterm16`].
/// Faithful to `struct HCL` in `colors.cpp` (`h`, `c`, `l` fields).
struct Hcl {
    h: u8,
    c: u8,
    l: u8,
}

/// HUE_PRECISION = 32 (colors.cpp)
const HUE_PRECISION: u8 = 32;
/// HUE_MAX = 6 * HUE_PRECISION = 192 (colors.cpp)
const HUE_MAX: u8 = 6 * HUE_PRECISION; // 192

/// Port of `RGBtoHCL` (`colors.cpp`).
///
/// The hue-angle arithmetic uses `i16` to match the C++ `int16_t(HUE_PRECISION*(G-B))/C`
/// semantics. `HUE_PRECISION * 255 = 8160` fits in `i16`, so no truncation occurs
/// for valid inputs, but we mirror the type exactly.
const fn rgb_to_hcl(r: u8, g: u8, b: u8) -> Hcl {
    // min/max are not available as const fn in std; inline with if/else.
    let xmin = if r < g {
        if r < b { r } else { b }
    } else {
        if g < b { g } else { b }
    };
    let xmax = if r > g {
        if r > b { r } else { b }
    } else {
        if g > b { g } else { b }
    };

    let v = xmax;
    // C++: uint8_t L = uint16_t(Xmax + Xmin) / 2
    // Use u16 intermediate to avoid overflow before the divide.
    let l = ((xmax as u16 + xmin as u16) / 2) as u8;
    let c = xmax - xmin; // safe: xmax >= xmin

    let h: i16 = if c != 0 {
        // C++ selects the first matching arm (when R==G, V==R wins)
        let raw: i16 = if v == r {
            // int16_t(HUE_PRECISION * (G - B)) / C
            (HUE_PRECISION as i16 * (g as i16 - b as i16)) / c as i16
        } else if v == g {
            // int16_t(HUE_PRECISION * (B - R)) / C + 2 * HUE_PRECISION
            (HUE_PRECISION as i16 * (b as i16 - r as i16)) / c as i16 + 2 * HUE_PRECISION as i16
        } else {
            // v == b
            // int16_t(HUE_PRECISION * (R - G)) / C + 4 * HUE_PRECISION
            (HUE_PRECISION as i16 * (r as i16 - g as i16)) / c as i16 + 4 * HUE_PRECISION as i16
        };
        if raw < 0 {
            raw + HUE_MAX as i16
        } else if raw >= HUE_MAX as i16 {
            raw - HUE_MAX as i16
        } else {
            raw
        }
    } else {
        0
    };

    Hcl { h: h as u8, c, l }
}

// ---------------------------------------------------------------------------
// Threshold constants derived from `constexpr uint8_t u8(double d)` (colors.cpp)
// The C++ helper casts d*255 to uint8_t (truncates toward zero):
//   u8(0.25)  = 63
//   u8(0.5)   = 127
//   u8(0.625) = 159
//   u8(0.875) = 223
//   u8(0.925) = 235
// We bake these as integer literals to avoid floating-point in const fn.
// ---------------------------------------------------------------------------
const THRESH_025: u8 = 63;
const THRESH_050: u8 = 127;
const THRESH_0625: u8 = 159;
const THRESH_0875: u8 = 223;
const THRESH_0925: u8 = 235;

// ---------------------------------------------------------------------------
// Public API
// ---------------------------------------------------------------------------

/// Port of `RGBtoXTerm16` (`source/platform/colors.cpp`).
///
/// Converts an RGB triplet to the nearest xterm-16 / BIOS-style index (0..=15).
/// Uses the HCL colour model for perceptually plausible quantization.
pub const fn rgb_to_xterm16(r: u8, g: u8, b: u8) -> u8 {
    let c = rgb_to_hcl(r, g, b);
    if c.c >= 12 {
        // Chromatic: pick the hue sector.
        // C++: index = (h + HUE_PRECISION/2, wrapping to [0,HUE_MAX)) / HUE_PRECISION
        let adjusted_h = if (c.h as u16) < (HUE_MAX as u16 - HUE_PRECISION as u16 / 2) {
            c.h + HUE_PRECISION / 2
        } else {
            // wraps: c.h - (HUE_MAX - HUE_PRECISION/2)
            // C++ subtracts so it stays in 0..HUE_MAX; use wrapping sub on u8 —
            // the value is always < HUE_MAX, so no actual wrap occurs for valid inputs.
            c.h - (HUE_MAX - HUE_PRECISION / 2)
        };
        let index = (adjusted_h / HUE_PRECISION) as usize;

        #[rustfmt::skip]
        const NORMAL: [u8; 6] = [0x1, 0x3, 0x2, 0x6, 0x4, 0x5];
        #[rustfmt::skip]
        const BRIGHT: [u8; 6] = [0x9, 0xB, 0xA, 0xE, 0xC, 0xD];

        if c.l < THRESH_050 {
            NORMAL[index]
        } else if c.l < THRESH_0925 {
            BRIGHT[index]
        } else {
            15
        }
    } else {
        // Achromatic: pick by lightness only.
        if c.l < THRESH_025 {
            0
        } else if c.l < THRESH_0625 {
            8
        } else if c.l < THRESH_0875 {
            7
        } else {
            15
        }
    }
}

/// Port of `RGBtoXTerm256` (`include/tvision/colors.h`).
///
/// Maps an RGB triplet to the nearest xterm-256 index **in the range 16..=255**.
/// The result is never in 0..=15 (the xterm-16 palette), which matches the C++
/// guarantee.
///
/// The inner `cnvColor` quantizes to the 6×6×6 cube; if the round-trip
/// `XTerm256toRGB(idx)` doesn't match the input, the colour is too achromatic
/// and `cnvGray` falls back to the 24-step grayscale ramp.
///
/// **Branchless dark-compensation** in `scale`: `c += 20 & -(c < 75)` in C++ is
/// equivalent to `if c < 75 { c + 20 } else { c }` (c+20 < 95 ≤ 127, no u8
/// overflow).
pub fn rgb_to_xterm256(r: u8, g: u8, b: u8) -> u8 {
    // scale: map a single channel to a cube index 0..=5.
    let scale = |c: u8| -> u8 {
        // Dark-compensation: add 20 iff c < 75 (branchless in C++; same semantics).
        let c = if c < 75 { c.wrapping_add(20) } else { c };
        // max(c, 35) - 35 then divide by 40, truncating (matches C++ uchar arithmetic).
        // saturating_sub is exactly max(c,35)-35 for u8.
        let c = c.saturating_sub(35);
        c / 40
    };

    // cnvColor: pack to cube index
    let ri = scale(r);
    let gi = scale(g);
    let bi = scale(b);
    // 16 + r*36 + g*6 + b, rewritten as (r*6 + g)*6 + b to match C++
    let idx = 16u8 + (ri * 6 + gi) * 6 + bi;

    // cnvGray: map a lightness value to the 24-step grayscale ramp.
    let cnv_gray = |l: u8| -> u8 {
        if l < 3 {
            // l < 8-5 → totally black → cube index 16 (= black in the 6x6x6 cube)
            16
        } else if l >= 243 {
            // l >= 238+5 → totally white → cube index 231 (= white)
            231
        } else {
            // 232 + (max(l, 3) - 3) / 10
            232 + (l - 3) / 10
        }
    };

    // If the cube quantization round-trips exactly, use it; otherwise fall back
    // to grayscale when the colour is achromatic (low chroma or idx==16).
    let packed = XTERM256_TO_RGB[idx as usize];
    let (qr, qg, qb) = (
        ((packed >> 16) & 0xFF) as u8,
        ((packed >> 8) & 0xFF) as u8,
        (packed & 0xFF) as u8,
    );
    if r == qr && g == qg && b == qb {
        idx
    } else {
        let xmin = r.min(g).min(b);
        let xmax = r.max(g).max(b);
        let chroma = xmax - xmin;
        if chroma < 12 || idx == 16 {
            let l = ((xmax as u16 + xmin as u16) / 2) as u8;
            cnv_gray(l)
        } else {
            idx
        }
    }
}

/// Port of `BIOStoXTerm16` / `XTerm16toBIOS` (`include/tvision/colors.h`).
///
/// Swaps the red and blue bits of a 4-bit BIOS colour index.
/// BIOS bit layout: bit0=blue, bit1=green, bit2=red, bit3=bright.
/// xterm-16 layout: bit0=red,  bit1=green, bit2=blue, bit3=bright.
///
/// The swap is `(x & 0b1010) | ((x & 1) << 2) | ((x >> 2) & 1)`.
///
/// C++ note: `BIOStoXTerm16` and `XTerm16toBIOS` both do this identical swap —
/// it is its own inverse.
pub const fn bios_to_xterm16(bios: u8) -> u8 {
    (bios & 0b1010) | ((bios & 1) << 2) | ((bios >> 2) & 1)
}

/// Port of `XTerm16toBIOS` (`include/tvision/colors.h`).
///
/// Identical bit-swap as [`bios_to_xterm16`] — the function is its own inverse.
/// C++ literally implements it as `BIOStoXTerm16(idx)`.
pub const fn xterm16_to_bios(idx: u8) -> u8 {
    bios_to_xterm16(idx)
}

/// Port of `RGBtoBIOS` (`include/tvision/colors.h`).
///
/// Converts an RGB triplet directly to a BIOS colour index (0..=15).
/// Equivalent to `xterm16_to_bios(rgb_to_xterm16(r, g, b))`.
pub const fn rgb_to_bios(r: u8, g: u8, b: u8) -> u8 {
    xterm16_to_bios(rgb_to_xterm16(r, g, b))
}

/// Port of `XTerm256toXTerm16` (`include/tvision/colors.h`).
///
/// LUT lookup into [`XTERM256_TO_XTERM16`]. For indices 0..=15 the LUT contains
/// the identity (the first 16 xterm-256 entries *are* the xterm-16 palette).
pub const fn xterm256_to_xterm16(idx: u8) -> u8 {
    XTERM256_TO_XTERM16[idx as usize]
}

/// Port of `XTerm256toRGB` (`include/tvision/colors.h`).
///
/// LUT lookup into [`XTERM256_TO_RGB`]. Valid for indices 16..=255 (the
/// 6×6×6 cube and 24-step grayscale ramp). Indices 0..=15 return `(0,0,0)` —
/// they are never looked up via this path in the C++ (the caller already has
/// the exact RGB from the palette).
///
/// Returns `(red, green, blue)`.
pub const fn xterm256_to_rgb(idx: u8) -> (u8, u8, u8) {
    let packed = XTERM256_TO_RGB[idx as usize];
    let r = ((packed >> 16) & 0xFF) as u8;
    let g = ((packed >> 8) & 0xFF) as u8;
    let b = (packed & 0xFF) as u8;
    (r, g, b)
}

// ---------------------------------------------------------------------------
// Compile-time LUT builders
// ---------------------------------------------------------------------------

/// xterm-256 cube channel values for indices 0..=5.
/// `i=0 → 0`, `i=1..5 → 55 + i*40` (i.e. 0, 95, 135, 175, 215, 255).
/// Faithful to the C++ comment in `colors.cpp`.
const fn cube_channel(i: u8) -> u8 {
    if i == 0 { 0 } else { 55 + i * 40 }
}

/// Build `XTERM256_TO_XTERM16`: for each xterm-256 index, the closest xterm-16
/// colour.
///
/// - Indices 0..=15: identity (those entries *are* the xterm-16 palette).
/// - Indices 16..=231: 6×6×6 cube — `rgb_to_xterm16(R, G, B)`.
/// - Indices 232..=255: 24-step grayscale — `rgb_to_xterm16(L, L, L)`.
const fn build_xterm256_to_xterm16() -> [u8; 256] {
    let mut table = [0u8; 256];

    // Indices 0..15: identity.
    let mut i = 0u8;
    while i < 16 {
        table[i as usize] = i;
        i += 1;
    }

    // Indices 16..231: 6×6×6 cube.
    let mut ri = 0u8;
    while ri < 6 {
        let mut gi = 0u8;
        while gi < 6 {
            let mut bi = 0u8;
            while bi < 6 {
                let r = cube_channel(ri);
                let g = cube_channel(gi);
                let b = cube_channel(bi);
                let idx = 16 + (ri * 6 + gi) * 6 + bi;
                table[idx as usize] = rgb_to_xterm16(r, g, b);
                bi += 1;
            }
            gi += 1;
        }
        ri += 1;
    }

    // Indices 232..255: 24-step grayscale ramp. L = i*10 + 8.
    let mut i = 0u8;
    while i < 24 {
        let l = i * 10 + 8;
        table[(232 + i) as usize] = rgb_to_xterm16(l, l, l);
        i += 1;
    }

    table
}

/// Build `XTERM256_TO_RGB`: for each xterm-256 index, the packed `0xRRGGBB`
/// value.
///
/// - Indices 0..=15: left as 0 (never looked up via `xterm256_to_rgb` in C++).
/// - Indices 16..=231: 6×6×6 cube.
/// - Indices 232..=255: 24-step grayscale ramp.
const fn build_xterm256_to_rgb() -> [u32; 256] {
    let mut table = [0u32; 256];

    // 6×6×6 cube.
    let mut ri = 0u8;
    while ri < 6 {
        let mut gi = 0u8;
        while gi < 6 {
            let mut bi = 0u8;
            while bi < 6 {
                let r = cube_channel(ri);
                let g = cube_channel(gi);
                let b = cube_channel(bi);
                let idx = 16 + (ri * 6 + gi) * 6 + bi;
                table[idx as usize] = ((r as u32) << 16) | ((g as u32) << 8) | (b as u32);
                bi += 1;
            }
            gi += 1;
        }
        ri += 1;
    }

    // Grayscale ramp.
    let mut i = 0u8;
    while i < 24 {
        let l = i * 10 + 8;
        table[(232 + i) as usize] = ((l as u32) << 16) | ((l as u32) << 8) | (l as u32);
        i += 1;
    }

    table
}

/// LUT: xterm-256 index → nearest xterm-16 index.
///
/// Built at compile time from [`rgb_to_xterm16`]. Indices 0..=15 map to
/// themselves; 16..=231 are the 6×6×6 cube; 232..=255 are the grayscale ramp.
/// (Faithful to `XTERM256_TO_XTERM16` in `colors.cpp`.)
pub const XTERM256_TO_XTERM16: [u8; 256] = build_xterm256_to_xterm16();

/// LUT: xterm-256 index → packed `0xRRGGBB`.
///
/// Built at compile time. Indices 0..=15 are 0 (never looked up via
/// [`xterm256_to_rgb`] in C++). 16..=231 are the 6×6×6 cube;
/// 232..=255 are the 24-step grayscale ramp.
/// (Faithful to `XTERM256_TO_RGB` in `colors.cpp`.)
pub const XTERM256_TO_RGB: [u32; 256] = build_xterm256_to_rgb();

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    // --- xterm256_to_rgb ---

    #[test]
    fn xterm256_to_rgb_cube_black() {
        // Index 16 = (0,0,0) in the 6×6×6 cube.
        assert_eq!(xterm256_to_rgb(16), (0, 0, 0));
    }

    #[test]
    fn xterm256_to_rgb_cube_white() {
        // Index 231 = (255,255,255) — last entry of the cube.
        assert_eq!(xterm256_to_rgb(231), (255, 255, 255));
    }

    #[test]
    fn xterm256_to_rgb_mid_cube() {
        // Index 16 + (3*6+3)*6+3 = 16 + 117 = 133.
        // ri=3 → 175, gi=3 → 175, bi=3 → 175 (all mid-cube).
        let idx = 16 + (3 * 6 + 3) * 6 + 3;
        assert_eq!(xterm256_to_rgb(idx), (175, 175, 175));
    }

    #[test]
    fn xterm256_to_rgb_grayscale_first() {
        // Index 232: L = 0*10+8 = 8.
        assert_eq!(xterm256_to_rgb(232), (8, 8, 8));
    }

    #[test]
    fn xterm256_to_rgb_grayscale_last() {
        // Index 255: L = 23*10+8 = 238.
        assert_eq!(xterm256_to_rgb(255), (238, 238, 238));
    }

    // --- cube channel values ---

    #[test]
    fn cube_channels_are_exactly_correct() {
        let expected = [0u8, 95, 135, 175, 215, 255];
        for (i, &expected_val) in expected.iter().enumerate() {
            assert_eq!(cube_channel(i as u8), expected_val, "cube_channel({i})");
        }
    }

    // --- bios/xterm16 bit-swap ---

    #[test]
    fn bios_xterm16_swap_is_involution() {
        // bios_to_xterm16 ∘ bios_to_xterm16 == identity for all 4-bit values.
        for x in 0u8..=15 {
            assert_eq!(
                bios_to_xterm16(bios_to_xterm16(x)),
                x,
                "involution failed for {x}"
            );
        }
    }

    #[test]
    fn bios_xterm16_swap_spot_check() {
        // BIOS red = 0b0100 (bit2=red) ↔ xterm-16 red = 0b0001 (bit0=red).
        // BIOS blue = 0b0001 (bit0=blue) ↔ xterm-16 blue = 0b0100 (bit2=blue).
        assert_eq!(bios_to_xterm16(0b0100), 0b0001); // BIOS red → xterm16 red
        assert_eq!(bios_to_xterm16(0b0001), 0b0100); // BIOS blue → xterm16 blue
        // Bright bit (bit3) must be preserved.
        assert_eq!(bios_to_xterm16(0b1100), 0b1001); // bright BIOS red → bright xterm16 red
        assert_eq!(bios_to_xterm16(0b1001), 0b1100); // bright BIOS blue → bright xterm16 blue
        // Green (bit1) is unchanged by the swap.
        assert_eq!(bios_to_xterm16(0b0010), 0b0010);
    }

    #[test]
    fn xterm16_to_bios_is_same_as_bios_to_xterm16() {
        for x in 0u8..=15 {
            assert_eq!(xterm16_to_bios(x), bios_to_xterm16(x));
        }
    }

    // --- rgb_to_xterm16 ---

    #[test]
    fn rgb_to_xterm16_pure_colors() {
        // Black and white (achromatic).
        assert_eq!(rgb_to_xterm16(0, 0, 0), 0); // black
        assert_eq!(rgb_to_xterm16(255, 255, 255), 15); // bright white

        // Pure red, green, blue.
        // xterm-16 uses the standard VGA layout: bit0=red, bit1=green, bit2=blue,
        // bit3=bright.  So: bright-red=0x9, bright-green=0xA, bright-blue=0xC.
        // (Note: xterm-16 != BIOS.  The bit-swap only applies to the BIOS variant.)
        assert_eq!(rgb_to_xterm16(255, 0, 0), 0x9); // bright red   (BRIGHT[0])
        assert_eq!(rgb_to_xterm16(0, 255, 0), 0xA); // bright green (BRIGHT[2])
        assert_eq!(rgb_to_xterm16(0, 0, 255), 0xC); // bright blue  (BRIGHT[4])

        // Mid gray — achromatic, should map to 8 (dark gray) or 7 (light gray).
        let mid_gray = rgb_to_xterm16(128, 128, 128);
        assert!(
            mid_gray == 8 || mid_gray == 7,
            "mid gray {mid_gray} unexpected"
        );
    }

    #[test]
    fn rgb_to_xterm16_gray_thresholds() {
        // l < 63 → 0 (black)
        assert_eq!(rgb_to_xterm16(40, 40, 40), 0);
        // 63 <= l < 159 → 8 (dark gray)
        assert_eq!(rgb_to_xterm16(100, 100, 100), 8);
        // 159 <= l < 223 → 7 (light gray)
        assert_eq!(rgb_to_xterm16(180, 180, 180), 7);
        // l >= 223 → 15 (bright white)
        assert_eq!(rgb_to_xterm16(230, 230, 230), 15);
    }

    // --- rgb_to_bios ---

    #[test]
    fn rgb_to_bios_matches_composition() {
        // rgb_to_bios == xterm16_to_bios(rgb_to_xterm16(...)) for all test colors.
        let cases = [
            (0u8, 0u8, 0u8),
            (255, 0, 0),
            (0, 255, 0),
            (0, 0, 255),
            (255, 255, 255),
            (128, 128, 128),
            (200, 100, 50),
        ];
        for (r, g, b) in cases {
            let expected = xterm16_to_bios(rgb_to_xterm16(r, g, b));
            assert_eq!(rgb_to_bios(r, g, b), expected, "rgb_to_bios({r},{g},{b})");
        }
    }

    // --- rgb_to_xterm256 ---

    #[test]
    fn rgb_to_xterm256_always_ge_16() {
        // Spot-check several values including edge cases.
        let cases = [
            (0u8, 0u8, 0u8),
            (255, 255, 255),
            (128, 0, 0),
            (0, 128, 0),
            (0, 0, 128),
            (10, 10, 10),   // dark, exercises grayscale fallback
            (255, 128, 64), // chromatic, stays in cube
        ];
        for (r, g, b) in cases {
            let idx = rgb_to_xterm256(r, g, b);
            assert!(idx >= 16, "rgb_to_xterm256({r},{g},{b}) = {idx} < 16");
        }
    }

    #[test]
    fn rgb_to_xterm256_dark_compensation() {
        // A channel value < 75 gets +20 added before scaling — exercise this path.
        // (50,50,50): all channels < 75, gets +20 → (70,70,70). scale(70): 70>35 → 35; 35/40=0.
        // So cube index = 16+(0*6+0)*6+0 = 16.
        // Round-trip: xterm256_to_rgb(16) = (0,0,0) ≠ (50,50,50).
        // Chroma = 0 < 12 → grayscale: L=50, cnvGray(50) = 232 + (50-3)/10 = 232+4 = 236.
        let idx = rgb_to_xterm256(50, 50, 50);
        assert!(idx >= 232, "expected grayscale idx, got {idx}");
    }

    #[test]
    fn rgb_to_xterm256_grayscale_fallback_black() {
        // Pure black: cube gives idx=16, round-trip fails, chroma=0 → cnvGray(0) → 16.
        let idx = rgb_to_xterm256(0, 0, 0);
        assert_eq!(idx, 16);
    }

    #[test]
    fn rgb_to_xterm256_grayscale_fallback_white() {
        // Pure white: cube gives idx=231, round-trip succeeds → stays in cube.
        let idx = rgb_to_xterm256(255, 255, 255);
        assert_eq!(idx, 231);
    }

    #[test]
    fn rgb_to_xterm256_chromatic_stays_in_cube() {
        // A strongly chromatic color: pure red (255,0,0).
        // scale(255): 255 >= 75, no compensation; 255-35=220; 220/40=5. ri=5.
        // scale(0): 0 < 75 → +20=20; max(20,35)=35; 35-35=0; 0/40=0. gi=0, bi=0.
        // idx = 16 + (5*6+0)*6+0 = 16+180 = 196.
        // xterm256_to_rgb(196) = (cube_channel(5), cube_channel(0), cube_channel(0)) = (255,0,0).
        // Round-trip matches → stays at 196.
        let idx = rgb_to_xterm256(255, 0, 0);
        assert_eq!(idx, 196);
    }

    // --- xterm256_to_xterm16 identity for 0..16 ---

    #[test]
    fn xterm256_to_xterm16_identity_for_low_indices() {
        for i in 0u8..16 {
            assert_eq!(xterm256_to_xterm16(i), i, "identity failed for {i}");
        }
    }
}
