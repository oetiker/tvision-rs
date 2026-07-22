//! Named layout metrics for dialogs — the recovered classic Turbo Vision
//! conventions (confirmed against `msgbox.cpp`/`tfildlg.cpp`), so dialogs stop
//! inventing their own coordinates. See `docs/design/dialog-layout.md`.

use crate::view::Point;

/// Standard button: 10 columns × 2 rows (row 2 is the drop shadow).
///
/// This is the default minimum face width for
/// [`button_row`](crate::dialog::Dialog::button_row); the autosize
/// [`ButtonLayout`](crate::dialog::ButtonLayout)s widen past it when a label needs
/// the room. Ten columns is what the conventional labels come to anyway — "Cancel"
/// is 6 columns plus [`BUTTON_TEXT_PADDING`].
pub const STD_BUTTON: Point = Point::new(10, 2);
/// Cells between adjacent buttons in a button row.
pub const BUTTON_GAP: i32 = 2;
/// Content inset from the left frame.
pub const MARGIN_LEFT: i32 = 3;
/// Content inset from the right frame.
pub const MARGIN_RIGHT: i32 = 2;
/// Content inset from the top frame.
pub const MARGIN_TOP: i32 = 2;
/// Button-row top edge = `dialog_height - BUTTON_ROW_FROM_BOTTOM`.
pub const BUTTON_ROW_FROM_BOTTOM: i32 = 3;
/// Columns a button face needs beyond its label.
///
/// [`Button::draw`](crate::widgets::Button) puts the title at column 2 and the
/// drop-shadow glyph at `width - 1`, so a label of `n` columns needs a face of
/// `n + 4` to keep one blank column between the text and the shadow. A narrower
/// face still renders, but the label butts against the shadow.
pub(crate) const BUTTON_TEXT_PADDING: i32 = 4;

/// The natural face width for a single label: its display width plus
/// [`BUTTON_TEXT_PADDING`], before any minimum is applied.
///
/// Measured with [`crate::text::cstrlen`], the same `~`-stripping measure
/// `Button::draw` centers with; measuring differently would size the face against
/// text the button lays out to a different width.
pub(crate) fn button_face_width(title: &str) -> i32 {
    crate::text::cstrlen(title) + BUTTON_TEXT_PADDING
}

/// How [`Dialog::button_row`](crate::dialog::Dialog::button_row) sizes the faces
/// in a row. In every case the width is floored at the dialog's configured
/// minimum (see [`set_button_min_width`](crate::dialog::Dialog::set_button_min_width),
/// default [`STD_BUTTON`]).
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
#[non_exhaustive]
pub enum ButtonLayout {
    /// Every face exactly the minimum width — the minimum is both floor *and*
    /// ceiling, so labels never widen a face. The classic Turbo Vision fixed row.
    /// This is the default.
    #[default]
    Classic,
    /// Every face one shared width: the widest label's natural width, floored at
    /// the minimum. A uniform row whose long labels still fit.
    Uniform,
    /// Each face sized to its own label's natural width, floored at the minimum.
    /// A ragged row with the tightest horizontal footprint.
    Ragged,
}

/// How [`Dialog::button_row`](crate::dialog::Dialog::button_row) places buttons.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ButtonRowAlign {
    /// Centered (message-box convention).
    Center,
    /// Right-grouped, ending [`MARGIN_RIGHT`] from the right frame (action dialogs).
    Right,
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A conventional label's natural face is the classic 10: "Cancel" is 6
    /// columns, `6 + 4 = 10` — which is why the recovered TV dialogs look right at
    /// `STD_BUTTON` in the first place.
    #[test]
    fn button_face_width_of_a_classic_label_is_std_button() {
        assert_eq!(button_face_width("~C~ancel"), 10);
    }

    /// A long label's natural face grows: "Keep editing" is 12 columns → 12 + 4.
    #[test]
    fn button_face_width_grows_with_the_label() {
        assert_eq!(button_face_width("~K~eep editing"), 16);
    }

    /// The `~` hotkey markers are layout-invisible and must not inflate the face.
    #[test]
    fn button_face_width_ignores_hotkey_markers() {
        assert_eq!(button_face_width("~D~iscard"), button_face_width("Discard"));
    }

    /// Layout defaults to Classic, so a plain Dialog is byte-for-byte unchanged.
    #[test]
    fn button_layout_defaults_to_classic() {
        assert_eq!(ButtonLayout::default(), ButtonLayout::Classic);
    }

    #[test]
    fn metrics_match_recovered_tv() {
        assert_eq!(STD_BUTTON, Point::new(10, 2));
        assert_eq!(
            (
                BUTTON_GAP,
                MARGIN_LEFT,
                MARGIN_RIGHT,
                MARGIN_TOP,
                BUTTON_ROW_FROM_BOTTOM
            ),
            (2, 3, 2, 2, 3)
        );
    }
}
