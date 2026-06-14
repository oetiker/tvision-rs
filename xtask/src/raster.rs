//! Rasterize a grid of styled cells (from [`crate::ansi_html::parse_grid`]) to
//! an RGBA image, using a bundled DejaVu Sans Mono font. The cell box is sized
//! from the font's own advance/line metrics so box-drawing glyphs (┌─┐│└┘═║…)
//! tile seamlessly.

use ab_glyph::{Font, FontRef, PxScale, ScaleFont};
use image::{Rgba, RgbaImage};

use crate::ansi_html::Cell;

const FONT_REGULAR: &[u8] = include_bytes!("../assets/DejaVuSansMono.ttf");
const FONT_BOLD: &[u8] = include_bytes!("../assets/DejaVuSansMono-Bold.ttf");

/// Font pixel size. 18px gives a crisp, readable terminal at 2x-ish zoom.
const PX: f32 = 18.0;

/// Fixed cell metrics + the loaded fonts, derived once and reused per frame.
pub struct Renderer {
    regular: FontRef<'static>,
    bold: FontRef<'static>,
    cell_w: u32,
    cell_h: u32,
    /// Per-glyph scale, stretched so the advance maps to exactly `cell_w` and the
    /// `ascent - descent` span maps to exactly `cell_h`. This sub-pixel correction
    /// is what makes box-drawing glyphs reach the integer cell edges and tile
    /// seamlessly (a uniform PX leaves a ~1px gap where the natural advance/line
    /// falls short of the rounded cell).
    scale: PxScale,
    ascent: f32,
}

impl Renderer {
    pub fn new() -> Self {
        let regular = FontRef::try_from_slice(FONT_REGULAR).expect("regular font parses");
        let bold = FontRef::try_from_slice(FONT_BOLD).expect("bold font parses");
        let scaled = regular.as_scaled(PxScale::from(PX));
        // Monospace: every glyph shares one advance.
        let adv = scaled.h_advance(regular.glyph_id('M'));
        let asc = scaled.ascent();
        let line = asc - scaled.descent(); // descent is negative
        let cell_w = adv.round().max(1.0) as u32;
        let cell_h = line.round().max(1.0) as u32; // no line_gap — box-drawing must fill the cell
        // Stretch glyphs so advance == cell_w and line == cell_h exactly.
        let scale = PxScale {
            x: PX * cell_w as f32 / adv,
            y: PX * cell_h as f32 / line,
        };
        let ascent = asc * cell_h as f32 / line;
        Renderer {
            regular,
            bold,
            cell_w,
            cell_h,
            scale,
            ascent,
        }
    }

    /// Render one frame. `cols`/`rows` fix the image size so every frame in an
    /// animation has identical dimensions even if a captured row is short.
    pub fn render(&self, grid: &[Vec<Cell>], cols: u32, rows: u32) -> RgbaImage {
        let w = self.cell_w * cols;
        let h = self.cell_h * rows;
        let mut img = RgbaImage::from_pixel(w, h, Rgba([0, 0, 0, 255]));

        for (ry, row) in grid.iter().enumerate().take(rows as usize) {
            for (cx, cell) in row.iter().enumerate().take(cols as usize) {
                let x0 = cx as u32 * self.cell_w;
                let y0 = ry as u32 * self.cell_h;

                // Cell background.
                for yy in 0..self.cell_h {
                    for xx in 0..self.cell_w {
                        img.put_pixel(
                            x0 + xx,
                            y0 + yy,
                            Rgba([cell.bg.0, cell.bg.1, cell.bg.2, 255]),
                        );
                    }
                }

                if cell.ch == ' ' || cell.ch == '\u{00a0}' {
                    continue;
                }

                // Box-drawing lines and block/shade elements: paint them ourselves
                // so they reach the exact integer cell edges and tile seamlessly
                // (font glyphs leave a ~1px anti-aliased gap at cell boundaries —
                // visible on frames, window shadows, and colour-picker swatches).
                if let Some(spec) = box_spec(cell.ch) {
                    self.draw_box(&mut img, x0, y0, spec, cell.fg);
                    continue;
                }
                if self.draw_block(&mut img, x0, y0, cell.ch, cell.fg) {
                    continue;
                }

                let font = if cell.bold { &self.bold } else { &self.regular };
                let glyph = font.glyph_id(cell.ch).with_scale_and_position(
                    self.scale,
                    ab_glyph::point(x0 as f32, y0 as f32 + self.ascent),
                );
                if let Some(outline) = font.outline_glyph(glyph) {
                    let bounds = outline.px_bounds();
                    outline.draw(|gx, gy, coverage| {
                        let px = bounds.min.x as i32 + gx as i32;
                        let py = bounds.min.y as i32 + gy as i32;
                        if px < 0 || py < 0 || px as u32 >= w || py as u32 >= h {
                            return;
                        }
                        let blended = blend(cell.fg, cell.bg, coverage);
                        img.put_pixel(
                            px as u32,
                            py as u32,
                            Rgba([blended.0, blended.1, blended.2, 255]),
                        );
                    });
                }
            }
        }
        img
    }

    /// Paint a box-drawing line char as exact rectangles. `spec` is the weight of
    /// each arm `[up, right, down, left]` — 0 = none, 1 = single, 2 = double. All
    /// strokes share one thickness (so single and double lines look balanced), and
    /// arms run edge-to-centre and cross at the junction, so mixed connectors
    /// (`╤ ╢ ╪ …`) join cleanly and adjacent cells tile with no gap.
    fn draw_box(&self, img: &mut RgbaImage, x0: u32, y0: u32, spec: [u8; 4], fg: (u8, u8, u8)) {
        const T: i32 = 2; // stroke thickness
        const OFF: i32 = 2; // double-stroke offset from the centre line
        let [up, right, down, left] = spec;
        let (cw, ch) = (self.cell_w as i32, self.cell_h as i32);
        let (mx, my) = (cw / 2, ch / 2);
        let color = Rgba([fg.0, fg.1, fg.2, 255]);
        let mut rect = |xa: i32, xb: i32, ya: i32, yb: i32| {
            for yy in ya.max(0)..yb.min(ch) {
                for xx in xa.max(0)..xb.min(cw) {
                    img.put_pixel(x0 + xx as u32, y0 + yy as u32, color);
                }
            }
        };
        // Stroke centre-lines: a horizontal line has 1 or 2 rows; a vertical 1 or 2 cols.
        let hw = left.max(right); // horizontal weight
        let vw = up.max(down); // vertical weight
        let ycs: &[i32] = match hw {
            2 => &[my - OFF, my + OFF],
            1 => &[my],
            _ => &[],
        };
        let xcs: &[i32] = match vw {
            2 => &[mx - OFF, mx + OFF],
            1 => &[mx],
            _ => &[],
        };
        // Horizontal strokes: each spans from the left edge (if a left arm) to the
        // right edge (if a right arm), otherwise only to the crossing vertical(s).
        if hw > 0 {
            let xa = if left > 0 {
                0
            } else if !xcs.is_empty() {
                xcs.iter().min().unwrap() - T / 2
            } else {
                mx
            };
            let xb = if right > 0 {
                cw
            } else if !xcs.is_empty() {
                xcs.iter().max().unwrap() + T / 2
            } else {
                mx
            };
            for &yc in ycs {
                rect(xa, xb, yc - T / 2, yc - T / 2 + T);
            }
        }
        // Vertical strokes: span top/bottom edges (if up/down arms), else to the
        // crossing horizontal(s) so the junction fills.
        if vw > 0 {
            let ya = if up > 0 {
                0
            } else if !ycs.is_empty() {
                ycs.iter().min().unwrap() - T / 2
            } else {
                my
            };
            let yb = if down > 0 {
                ch
            } else if !ycs.is_empty() {
                ycs.iter().max().unwrap() + T / 2
            } else {
                my
            };
            for &xc in xcs {
                rect(xc - T / 2, xc - T / 2 + T, ya, yb);
            }
        }
    }
    /// Paint a block / shade element (U+2580..U+2595) as exact fills so window
    /// shadows, buttons and colour swatches tile seamlessly. Shades use a global
    /// dither so adjacent cells line up. Returns false if `ch` is not a block.
    fn draw_block(
        &self,
        img: &mut RgbaImage,
        x0: u32,
        y0: u32,
        ch: char,
        fg: (u8, u8, u8),
    ) -> bool {
        let (cw, chh) = (self.cell_w, self.cell_h);
        let color = Rgba([fg.0, fg.1, fg.2, 255]);
        let mut solid = |xa: u32, xb: u32, ya: u32, yb: u32| {
            for yy in ya..yb {
                for xx in xa..xb {
                    img.put_pixel(x0 + xx, y0 + yy, color);
                }
            }
        };
        let cp = ch as u32;
        match ch {
            '█' => solid(0, cw, 0, chh),
            '▀' => solid(0, cw, 0, chh / 2),
            '▐' => solid(cw / 2, cw, 0, chh),
            '▔' => solid(0, cw, 0, (chh / 8).max(1)),
            '▕' => solid(cw - (cw / 8).max(1), cw, 0, chh),
            '░' | '▒' | '▓' => {
                for yy in 0..chh {
                    for xx in 0..cw {
                        let (gx, gy) = (x0 + xx, y0 + yy);
                        let on = match ch {
                            '░' => gx % 2 == 0 && gy % 2 == 0,  // ~25%
                            '▒' => (gx + gy) % 2 == 0,          // ~50% checker
                            _ => !(gx % 2 == 1 && gy % 2 == 1), // ▓ ~75%
                        };
                        if on {
                            img.put_pixel(gx, gy, color);
                        }
                    }
                }
            }
            // Lower eighths ▁..▇ (and ▄ = lower half): fill the bottom n/8.
            _ if (0x2581..=0x2587).contains(&cp) => {
                let n = cp - 0x2580; // 1..7
                let filled = (chh * n / 8).max(1).min(chh);
                solid(0, cw, chh - filled, chh);
            }
            // Left eighths ▉..▏ (and ▌ = left half): fill the left n/8.
            _ if (0x2589..=0x258F).contains(&cp) => {
                let n = 0x2590 - cp; // 7..1
                let filled = (cw * n / 8).max(1).min(cw);
                solid(0, filled, 0, chh);
            }
            _ => return false,
        }
        true
    }
}

/// Arm weights `[up, right, down, left]` for a box-drawing line char: 0 = none,
/// 1 = single, 2 = double. Covers single, double, and mixed single/double
/// connectors. `None` ⇒ not a handled box char (fall back to the font).
fn box_spec(ch: char) -> Option<[u8; 4]> {
    Some(match ch {
        // singles
        '─' => [0, 1, 0, 1],
        '│' => [1, 0, 1, 0],
        '┌' => [0, 1, 1, 0],
        '┐' => [0, 0, 1, 1],
        '└' => [1, 1, 0, 0],
        '┘' => [1, 0, 0, 1],
        '├' => [1, 1, 1, 0],
        '┤' => [1, 0, 1, 1],
        '┬' => [0, 1, 1, 1],
        '┴' => [1, 1, 0, 1],
        '┼' => [1, 1, 1, 1],
        // doubles
        '═' => [0, 2, 0, 2],
        '║' => [2, 0, 2, 0],
        '╔' => [0, 2, 2, 0],
        '╗' => [0, 0, 2, 2],
        '╚' => [2, 2, 0, 0],
        '╝' => [2, 0, 0, 2],
        '╠' => [2, 2, 2, 0],
        '╣' => [2, 0, 2, 2],
        '╦' => [0, 2, 2, 2],
        '╩' => [2, 2, 0, 2],
        '╬' => [2, 2, 2, 2],
        // mixed single/double (used by single divider ↔ double frame joins)
        '╒' => [0, 2, 1, 0],
        '╓' => [0, 1, 2, 0],
        '╕' => [0, 0, 1, 2],
        '╖' => [0, 0, 2, 1],
        '╘' => [1, 2, 0, 0],
        '╙' => [2, 1, 0, 0],
        '╛' => [1, 0, 0, 2],
        '╜' => [2, 0, 0, 1],
        '╞' => [1, 2, 1, 0],
        '╟' => [2, 1, 2, 0],
        '╡' => [1, 0, 1, 2],
        '╢' => [2, 0, 2, 1],
        '╤' => [0, 2, 1, 2],
        '╥' => [0, 1, 2, 1],
        '╧' => [1, 2, 0, 2],
        '╨' => [2, 1, 0, 1],
        '╪' => [1, 2, 1, 2],
        '╫' => [2, 1, 2, 1],
        _ => return None,
    })
}

/// Alpha-blend `fg` over `bg` by `coverage` (0..=1).
fn blend(fg: (u8, u8, u8), bg: (u8, u8, u8), a: f32) -> (u8, u8, u8) {
    let mix = |f: u8, b: u8| {
        (f as f32 * a + b as f32 * (1.0 - a))
            .round()
            .clamp(0.0, 255.0) as u8
    };
    (mix(fg.0, bg.0), mix(fg.1, bg.1), mix(fg.2, bg.2))
}
