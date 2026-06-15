//! The always-visible info column: old/new swatches + the variant readout.
//! Reads the shared model; never switches with the tabs.

use super::SharedModel;
use super::model::color_to_display_rgb;
use crate::color::{Color, Style};
use crate::event::Event;
use crate::theme::Role;
use crate::view::{Context, DrawCtx, Rect, View, ViewState};

pub(crate) struct InfoColumn {
    state: ViewState,
    model: SharedModel,
    old: Color,
}

impl InfoColumn {
    pub(crate) fn new(bounds: Rect, model: SharedModel, old: Color) -> Self {
        InfoColumn {
            state: ViewState::new(bounds),
            model,
            old,
        }
    }
}

impl View for InfoColumn {
    fn state(&self) -> &ViewState {
        &self.state
    }
    fn state_mut(&mut self) -> &mut ViewState {
        &mut self.state
    }

    fn draw(&mut self, ctx: &mut DrawCtx) {
        let sz = self.state.size;
        // Gray dialog chrome — StaticText, not a blue Scroller role.
        let normal = ctx.style(Role::StaticText);
        let cur = self.model.borrow().color;

        // Column-local: fill the whole column.
        ctx.fill(Rect::new(0, 0, sz.x, sz.y), ' ', normal);

        // Old label + swatch.
        ctx.put_str(1, 0, "Old:", normal);
        let old_swatch = match color_to_display_rgb(self.old) {
            Some((r, g, b)) => Style::new(Color::Rgb(r, g, b), Color::Rgb(r, g, b)),
            None => normal,
        };
        ctx.fill(Rect::new(1, 1, 5, 2), ' ', old_swatch);

        // New label + swatch.
        ctx.put_str(1, 2, "New:", normal);
        let new_swatch = match color_to_display_rgb(cur) {
            Some((r, g, b)) => Style::new(Color::Rgb(r, g, b), Color::Rgb(r, g, b)),
            None => normal,
        };
        ctx.fill(Rect::new(1, 3, 5, 4), ' ', new_swatch);

        // Variant readout.
        let variant_str = match cur {
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
        ctx.put_str(1, 5, &variant_str, normal);
    }

    fn handle_event(&mut self, _ev: &mut Event, _ctx: &mut Context) {
        // Passive: the info column never consumes events.
    }
}
