//! Mouse-drag capture for the color picker (the `window.rs DragCapture` pattern).

use crate::capture::{CaptureFlow, CaptureHandler};
use crate::event::Event;
use crate::view::{Context, Point, ViewId};

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

/// The D9 drag capture for the color picker (the `window.rs DragCapture` analogue).
///
/// Holds the picker's id + the picker's **picker-local** origin (`body_origin`,
/// cached from the picker's last `draw` = the absolute screen pos of picker-local
/// `(0,0)`), so each absolute `MouseMove` converts to picker-local before posting
/// the broker request. The region being scrubbed lives in the picker's `active_drag`
/// field — neither this handler nor the `Deferred` variant carries a widget type.
pub(crate) struct ColorDragCapture {
    picker: ViewId,
    /// Absolute screen position of picker-local (0,0) — the picker's `body_origin`.
    origin: Point,
}

impl ColorDragCapture {
    pub(crate) fn new(picker: ViewId, origin: Point) -> Self {
        ColorDragCapture { picker, origin }
    }
}

impl CaptureHandler for ColorDragCapture {
    fn handle(&mut self, ev: &mut Event, ctx: &mut Context) -> CaptureFlow {
        match ev {
            Event::MouseMove(m) => {
                let local = m.position - self.origin; // abs → picker-local
                ctx.request_color_drag(self.picker, local);
                CaptureFlow::Consumed
            }
            Event::MouseUp(m) => {
                let local = m.position - self.origin;
                ctx.request_color_drag(self.picker, local);
                CaptureFlow::ConsumedPop
            }
            _ => CaptureFlow::Pass,
        }
    }

    fn view(&self) -> Option<ViewId> {
        Some(self.picker)
    }
}
