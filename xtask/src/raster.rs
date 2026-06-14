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
    ascent: f32,
}

impl Renderer {
    pub fn new() -> Self {
        let regular = FontRef::try_from_slice(FONT_REGULAR).expect("regular font parses");
        let bold = FontRef::try_from_slice(FONT_BOLD).expect("bold font parses");
        let scaled = regular.as_scaled(PxScale::from(PX));
        // Monospace: every glyph shares one advance. Size the cell from the
        // font's own metrics so adjacent box-drawing glyphs join up exactly.
        let cell_w = scaled.h_advance(regular.glyph_id('M')).round() as u32;
        let cell_h = (scaled.ascent() - scaled.descent() + scaled.line_gap()).round() as u32;
        let ascent = scaled.ascent();
        Renderer {
            regular,
            bold,
            cell_w,
            cell_h,
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

                let font = if cell.bold { &self.bold } else { &self.regular };
                let glyph = font.glyph_id(cell.ch).with_scale_and_position(
                    PX,
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
